use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    default,
    hash::Hash,
};

use futures::future::Join;
use log::info;
use ractor::{
    async_trait, cast, rpc::cast, Actor, ActorId, ActorName, ActorProcessingErr, ActorRef,
    RpcReplyPort,
};
use simple_logger::SimpleLogger;
use tokio::{
    task::JoinError,
    time::{Duration, Instant},
};

use crate::agent::{agent, Agent};

// ============================ Agent Actor ============================ //

enum ActorMessage {
    StartWork(ActorId),

    EndWork(ActorId),

    RestartWork(ActorId),

    GetStatus(ActorId),
    CurrentStatus(ActorId),

    /// Status messages
    /// Actor has not starting operator
    Idle(ActorId),
    /// Actor is Running
    Running(ActorId),

    /// Actor completed Successfully
    Success(ActorId),

    /// Actor failed due to an error
    Failed(ActorId),

    /// Metrics of the Actor
    SendMetrics(RpcReplyPort<AgentMetrics>),

    ///Check if the Actor has Compelted
    IsComplete(ActorRef<ActorMessage>),
    /// Completed
    Completed(ActorId),
    /// Not Completed
    NotCompleted(ActorId),
    /// Request the fork be sent to a philosopher
    // RequestFork(ActorRef<ActorInternalMessage>),
    /// Mark the fork as currently being used
    UsingFork(ActorId),
    /// Sent to a fork to indicate that it was put down and no longer is in use. This will
    /// allow the fork to be sent to the next user.
    PutForkDown(ActorId),
}

#[cfg(feature = "cluster")]
impl ractor::Message for ActorMessage {}

struct AgentArguments {
    before_name: VecDeque<String>,
    after_name: VecDeque<String>,
    agent: Box<dyn Agent>,
}
#[derive(Clone, Debug)]
struct AgentMetrics {
    /// The number of state changes that have occurred.
    state_change_count: u16,
    //TODO
}

struct AgentState {
    completed: bool,
    owned_by: ActorRef<ActorMessage>,
    backlog: VecDeque<ActorMessage>,
    before_name: VecDeque<String>,
    after_name: VecDeque<String>,
    metrics: AgentMetrics,
}

struct AgentActor {}

impl AgentActor {
    fn handle_internal(
        &self,
        myself: &ActorRef<ActorMessage>,
        message: ActorMessage,
        state: &mut AgentState,
    ) -> Option<ActorMessage> {
        match &message {
            ActorMessage::StartWork(who) => {
                if state.owned_by.get_id() == *who {
                    log::info!("Start Actor ");
                } else {
                    log::info!(
                        "ERROR Recieved StartWork {:?}. Real Owner is {:?}",
                        who,
                        state.owned_by.get_id()
                    );
                }
                None
            }
            ActorMessage::EndWork(who) => {
                if state.owned_by.get_id() == *who {
                    log::info!("End work for  Actor ");
                } else {
                    log::info!(
                        "ERROR Recieved EndWork {:?}. Real Owner is {:?}",
                        who,
                        state.owned_by.get_id()
                    );
                }
                None
            }
            ActorMessage::RestartWork(who) => {
                if state.owned_by.get_id() == *who {
                    // TODO update State.
                    // Inform to supervisor about restart
                    // Restart the actor.
                    log::info!("Restarting for  Actor ");
                    let _ = cast(myself, ActorMessage::StartWork(state.owned_by.get_id()));
                } else {
                    log::info!(
                        "ERROR Recieved EndWork {:?}. Real Owner is {:?}",
                        who,
                        state.owned_by.get_id()
                    );
                }
                None
            }
            _default => {
                log::info!("Unknown message is recieved");
                None
            }
        }
    }
}

#[async_trait]
impl Actor for AgentActor {
    type Msg = ActorMessage;
    type State = AgentState;
    type Arguments = AgentArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: AgentArguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        log::info!("Pre start api invoked");
        Ok(Self::State {
            owned_by: myself,
            backlog: VecDeque::new(),
            completed: false,
            before_name: args.before_name,
            after_name: args.after_name,
            metrics: AgentMetrics {
                state_change_count: 0,
            },
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        log::info!(
            "handle invoked {}",
            myself.get_name().unwrap_or("no name".to_string())
        );
        let mut maybe_unhandled = self.handle_internal(&myself, message, state);
        if let Some(message) = maybe_unhandled {
            state.backlog.push_back(message);
        } else {
            // we handled the message, check the queue for any work to dequeue and handle
            while !state.backlog.is_empty() && maybe_unhandled.is_none() {
                let head = state.backlog.pop_front().unwrap();
                maybe_unhandled = self.handle_internal(&myself, head, state);
            }
            // put the first unhandled msg back to the front of the queue
            if let Some(msg) = maybe_unhandled {
                state.backlog.push_front(msg);
            }
        }
        Ok(())
    }
}

// fn init_logging() {
//     let dir = tracing_subscriber::filter::Directive::from(tracing::Level::DEBUG);

//     use std::io::stderr;
//     use std::io::IsTerminal;
//     use tracing_glog::Glog;
//     use tracing_glog::GlogFields;
//     use tracing_subscriber::filter::EnvFilter;
//     use tracing_subscriber::layer::SubscriberExt;
//     use tracing_subscriber::Registry;

//     let fmt = tracing_subscriber::fmt::Layer::default()
//         .with_ansi(stderr().is_terminal())
//         .with_writer(std::io::stderr)
//         .event_format(Glog::default().with_timer(tracing_glog::LocalTime::default()))
//         .fmt_fields(GlogFields::default().compact());

//     let filter = vec![dir]
//         .into_iter()
//         .fold(EnvFilter::from_default_env(), |filter, directive| {
//             filter.add_directive(directive)
//         });

//     let subscriber = Registry::default().with(filter).with(fmt);
//     tracing::subscriber::set_global_default(subscriber).expect("to set global subscriber");
// }

struct AgentSystem {
    // TODO remove VecDeque and make it a vector.
    nodes_after: HashMap<String, VecDeque<String>>,
    nodes_before: HashMap<String, VecDeque<String>>,
    agent_map: HashMap<String, Box<dyn Agent>>,
    actors: HashMap<String, ActorRef<ActorMessage>>,
    all_handles: tokio::task::JoinSet<Result<(), JoinError>>,
}

const START_NODE: &str = "start";
const END_NODE: &str = "end";

impl AgentSystem {
    fn new() -> Self {
        let mut nodes_after: HashMap<String, VecDeque<String>> = HashMap::new();
        nodes_after.insert(START_NODE.into(), VecDeque::new());

        let mut nodes_before: HashMap<String, VecDeque<String>> = HashMap::new();
        nodes_before.insert(END_NODE.into(), VecDeque::new());
        Self {
            nodes_after,
            nodes_before,
            agent_map: HashMap::new(),
            actors: HashMap::new(),
            all_handles: tokio::task::JoinSet::new(),
        }
    }

    fn add_agent(&mut self, name: &str, agent: Box<dyn Agent>) -> &mut Self {
        if self.nodes_after.contains_key(name) {
            panic!("Agent with this name {} already exists", name);
        } else {
            self.add_entry(START_NODE.into(), name);
            self.agent_map.insert(name.into(), agent);
        }
        self
    }

    fn add_child(
        &mut self,
        parent_name: &str,
        child_name: &str,
        agent: Box<dyn Agent>,
    ) -> &mut Self {
        if !self.nodes_after.contains_key(parent_name) {
            panic!("Agent with parent name {} does not exist", parent_name);
        }
        self.add_entry(parent_name, child_name);
        self.agent_map.insert(child_name.into(), agent);

        self
    }

    // build the agent system.
    fn build(&mut self) -> &mut Self {
        self.add_end_entry();
        self.start();
        self
    }

    // Spawn all the actors and trigger start
    async fn start(&mut self) -> &Self {
        for (agent_name, agent) in self.agent_map.drain() {
            let (actor, handle) = Actor::spawn(
                Some(agent_name.clone()),
                AgentActor {},
                AgentArguments {
                    after_name: self.nodes_after.get(&agent_name).unwrap().clone(),
                    before_name: self.nodes_before.get(&agent_name).unwrap().clone(),
                    agent: agent,
                },
            )
            .await
            .expect("failed to create agent actor");
            self.actors.insert(agent_name, actor);
            self.all_handles.spawn(handle);
        }

        self
    }
    // helper method to add entry.
    fn add_entry(&mut self, parent: &str, child: &str) {
        // add entry for child
        self.nodes_after.insert(child.into(), VecDeque::new());

        self.nodes_before
            .entry(child.into())
            .or_insert_with(|| VecDeque::new())
            .push_back(parent.into());

        // add entry for parent
        self.nodes_after
            .entry(parent.into())
            .and_modify(|v| v.push_back(child.into()));
    }

    // helper method to add end entry
    fn add_end_entry(&mut self) {
        let mut end_nodes = Vec::new();
        for (k, v) in &self.nodes_after {
            if v.len() == 0 {
                end_nodes.push(k.clone())
            }
        }
        for name in end_nodes.drain(..) {
            let _ = &mut self.add_entry(&name, END_NODE);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, sync::Arc};

    use ollama_rs::models::create;
    use ractor::registry;
    use serde_json::Value;

    use crate::llm::ollama::client::Ollama;
    use crate::{
        agent::ConversationalAgentBuilder, memory::SimpleMemory, prompt_args, tools::Tool,
    };

    use super::*;

    #[derive(Clone)]
    struct Calc {}

    #[async_trait]
    impl Tool for Calc {
        fn name(&self) -> String {
            "Calculator".to_string()
        }
        fn description(&self) -> String {
            "Usefull to make calculations".to_string()
        }
        async fn run(&self, _input: Value) -> Result<String, Box<dyn Error>> {
            Ok("25".to_string())
        }
    }

    fn create_agent() -> Box<dyn Agent> {
        let llm = Ollama::default().with_model("llama3");
        let memory = SimpleMemory::new();
        let tool_calc = Calc {};
        let agent: crate::agent::ConversationalAgent = ConversationalAgentBuilder::new()
            .tools(&[Arc::new(tool_calc)])
            .build(llm)
            .unwrap();
        return Box::new(agent);
    }

    #[tokio::test]
    async fn test_agent_system_dag1() {
        let mut agent_system = AgentSystem::new();
        agent_system
            .add_agent("A", create_agent())
            .add_child("A", "B", create_agent())
            .add_child("A", "C", create_agent())
            .add_child("B", "D", create_agent())
            .add_child("C", "D", create_agent())
            .build();

        let mut exp_nodes_after: HashMap<String, VecDeque<String>> = HashMap::new();
        exp_nodes_after.insert("D".into(), VecDeque::from(vec![END_NODE.into()]));
        exp_nodes_after.insert("A".into(), VecDeque::from(vec!["B".into(), "C".into()]));
        exp_nodes_after.insert("C".into(), VecDeque::from(vec!["D".into()]));
        exp_nodes_after.insert(END_NODE.into(), VecDeque::from(vec![]));
        exp_nodes_after.insert("B".into(), VecDeque::from(vec!["D".into()]));
        exp_nodes_after.insert(START_NODE.into(), VecDeque::from(vec!["A".into()]));

        let mut exp_nodes_before: HashMap<String, VecDeque<String>> = HashMap::new();
        exp_nodes_before.insert(END_NODE.into(), VecDeque::from(vec!["D".into()]));
        exp_nodes_before.insert("D".into(), VecDeque::from(vec!["B".into(), "C".into()]));
        exp_nodes_before.insert("C".into(), VecDeque::from(vec!["A".into()]));
        exp_nodes_before.insert("A".into(), VecDeque::from(vec![START_NODE.into()]));
        exp_nodes_before.insert("B".into(), VecDeque::from(vec!["A".into()]));

        assert_eq!(agent_system.nodes_after, exp_nodes_after);
        assert_eq!(agent_system.nodes_before, exp_nodes_before);
        println!(
            "{:?}, {:?}",
            agent_system.nodes_after, agent_system.nodes_before
        );
    }

    #[tokio::test]
    async fn test_agent_system_start_single() {
        SimpleLogger::new().init().unwrap();
        let run_time = Duration::from_secs(5);

        let llm = Ollama::default().with_model("llama3");
        let memory = SimpleMemory::new();
        let tool_calc = Calc {};
        let agent: crate::agent::ConversationalAgent = ConversationalAgentBuilder::new()
            .tools(&[Arc::new(tool_calc)])
            .build(llm)
            .unwrap();
        let input_variables = prompt_args! {
            "input" => "hola,Me llamo luis, y tengo 10 anos, y estudio Computer scinence",
        };

        let mut wrapper = AgentSystem::new();
        let agent_system = wrapper
            .add_agent("A", Box::new(agent))
            .build()
            .start()
            .await;
        // let (actor, handle) = Actor::spawn(
        //     Some("agentA".into()),
        //     AgentActor {},
        //     AgentArguments {
        //         after_name: VecDeque::new(),
        //         before_name: VecDeque::new(),
        //         agent: Box::new(agent),
        //     },
        // )
        // .await
        // .expect("failed to create agent actor");
        // let r = registry::where_is("agentA".into());
        // if let Some(a_ref) = r {
        //     // let t = cast!(a_ref, ActorMessage::StartWork(a_ref.get_id()));
        //     let _ = a_ref.send_message(ActorMessage::StartWork(a_ref.get_id()));
        //     let _ = a_ref.send_message(ActorMessage::EndWork(a_ref.get_id()));
        // }

        // tokio::time::sleep(run_time).await;
        // actor.stop(None)
    }

    #[tokio::test]
    async fn test_agent_integration() {
        SimpleLogger::new().init().unwrap();
        let run_time = Duration::from_secs(5);

        let llm = Ollama::default().with_model("llama3");
        let memory = SimpleMemory::new();
        let tool_calc = Calc {};
        let agent: crate::agent::ConversationalAgent = ConversationalAgentBuilder::new()
            .tools(&[Arc::new(tool_calc)])
            .build(llm)
            .unwrap();
        let input_variables = prompt_args! {
            "input" => "hola,Me llamo luis, y tengo 10 anos, y estudio Computer scinence",
        };

        let (actor, handle) = Actor::spawn(
            Some("agentA".into()),
            AgentActor {},
            AgentArguments {
                after_name: VecDeque::new(),
                before_name: VecDeque::new(),
                agent: Box::new(agent),
            },
        )
        .await
        .expect("failed to create agent actor");
        let r = registry::where_is("agentA".into());
        if let Some(a_ref) = r {
            // let t = cast!(a_ref, ActorMessage::StartWork(a_ref.get_id()));
            let _ = a_ref.send_message(ActorMessage::StartWork(a_ref.get_id()));
            let _ = a_ref.send_message(ActorMessage::EndWork(a_ref.get_id()));
        }

        tokio::time::sleep(run_time).await;
        actor.stop(None)
    }
    #[tokio::test]
    async fn test_multi_agent() {
        SimpleLogger::new().init().unwrap();
        let llm = Ollama::default().with_model("llama3");
        let _memory = SimpleMemory::new();
        let tool_calc = Calc {};

        // TODO: move configuration to CLAP args
        let _time_slice = Duration::from_millis(10);
        let run_time = Duration::from_secs(5);

        let actor_names = ["A", "B", "C"];
        let mut actors = Vec::with_capacity(actor_names.len());
        let mut all_handles = tokio::task::JoinSet::new();

        for i in 0..actor_names.len() {
            let agent: crate::agent::ConversationalAgent = ConversationalAgentBuilder::new()
                .tools(&[Arc::new(tool_calc.clone())])
                .build(llm.clone())
                .unwrap();
            let agent_box = Box::new(agent);
            let (actor, handle) = Actor::spawn(
                Some(actor_names[i].into()),
                AgentActor {},
                AgentArguments {
                    after_name: VecDeque::new(),
                    before_name: VecDeque::new(),
                    agent: agent_box,
                },
            )
            .await
            .expect("failed to create agent actor");
            actors.push(actor);
            all_handles.spawn(handle);
        }
        let first_actor = actors[0].clone();
        let t = cast!(&first_actor, ActorMessage::StartWork(first_actor.get_id()));
        println!("**** **** **** *** ** *");
        println!("{:?}", t);
        // wait for the simulation to end
        tokio::time::sleep(run_time).await;
        for actor in actors {
            actor.stop(None)
        }
    }
}
