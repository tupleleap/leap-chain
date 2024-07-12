use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    default,
    hash::Hash,
};

use log::info;
use ractor::{
    async_trait, cast, rpc::cast, Actor, ActorId, ActorName, ActorProcessingErr, ActorRef,
    RpcReplyPort,
};
use simple_logger::SimpleLogger;
use tokio::time::{Duration, Instant};

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
    before: VecDeque<ActorRef<ActorMessage>>,
    after: VecDeque<ActorRef<ActorMessage>>,
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
    before: VecDeque<ActorRef<ActorMessage>>,
    after: VecDeque<ActorRef<ActorMessage>>,
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
            before: args.before,
            after: args.after,
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
    //     // start: Option<ActorRef<ActorMessage>>,
    //     // end: Option<ActorRef<ActorMessage>>,
    //     all_handles: tokio::task::JoinSet<()>,
    //     nodes: HashMap<ActorRef<()>, VecDeque<ActorRef<()>>>,
    nodes_after: HashMap<String, VecDeque<String>>,
    nodes_before: HashMap<String, VecDeque<String>>,
    agentMap: HashMap<String, Box<dyn Agent>>,
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
            agentMap: HashMap::new(),
        }
    }

    fn add_agent(&mut self, name: String, agent: Box<dyn Agent>) -> &mut Self {
        if self.nodes_after.contains_key(&name) {
            panic!("Agent with this name {} already exists", name);
        } else {
            self.add_entry(START_NODE.into(), name.clone());
            self.agentMap.insert(name, agent);
        }
        self
    }

    fn add_child(
        &mut self,
        parent_name: String,
        child_name: String,
        agent: Box<dyn Agent>,
    ) -> &mut Self {
        if !self.nodes_after.contains_key(&parent_name) {
            panic!("Agent with parent name {} does not exist", parent_name);
        }
        if self.nodes_after.contains_key(&child_name) {
            panic!("Agent with name {} already exists", child_name);
        }
        self.add_entry(parent_name, child_name.clone());
        self.agentMap.insert(child_name.clone(), agent);

        self
    }

    // helper method to add entry.
    fn add_entry(&mut self, parent: String, child: String) {
        // add entry for child
        self.nodes_after.insert(child.clone(), VecDeque::new());

        self.nodes_before
            .entry(child.clone())
            .or_insert_with(|| VecDeque::new())
            .push_back(parent.clone());

        // add entry for parent
        self.nodes_after
            .entry(parent.clone())
            .and_modify(|v| v.push_back(child.clone()));
    }

    // helper method to add end entry
    fn add_end_entry(&mut self) {
        let mut end_nodes: Vec<String> = Vec::new();
        for (k, v) in &self.nodes_after {
            if v.len() == 0 {
                end_nodes.push(k.into())
            }
        }
        for name in end_nodes.drain(..) {
            self.add_entry(name, END_NODE.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, sync::Arc};

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
                after: VecDeque::new(),
                before: VecDeque::new(),
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
                    after: VecDeque::new(),
                    before: VecDeque::new(),
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
