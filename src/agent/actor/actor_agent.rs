use std::collections::{HashMap, HashSet, VecDeque};

use ractor::{
    async_trait, cast, registry, rpc::cast, Actor, ActorProcessingErr, ActorRef, RpcReplyPort,
};
use serde_json::Value;
use tokio::task::JoinError;

use crate::{
    agent::{self, Agent, AgentExecutor, ConversationalAgent},
    chain::Chain,
    prompt_args,
};

// ============================ Agent Actor ============================ //

enum ActorMessage {
    StartWork(String, HashMap<String, Value>),

    EndWork(String, String),

    RestartWork(String, String),

    GetStatus(String),
    CurrentStatus(String),

    /// Status messages
    /// Actor has not starting operator
    Idle(String),
    /// Actor is Running
    Running(String),

    /// Actor completed Successfully
    Success(String),

    /// Actor failed due to an error
    Failed(String),

    /// Metrics of the Actor
    SendMetrics(RpcReplyPort<AgentMetrics>),

    ///Check if the Actor has Compelted
    IsComplete(ActorRef<ActorMessage>),
    /// Completed
    Completed(String),
    /// Not Completed
    NotCompleted(String),
}

#[cfg(feature = "cluster")]
impl ractor::Message for ActorMessage {}

struct AgentArguments {
    agent_name: String,
    before_name: HashSet<String>,
    after_name: HashSet<String>,
    agent: Option<Box<dyn Agent>>,
}
#[derive(Clone, Debug)]
struct AgentMetrics {
    /// The number of state changes that have occurred.
    state_change_count: u16,
    //TODO
}

struct AgentState {
    agent_name: String,
    completed: bool,
    owned_by: ActorRef<ActorMessage>,
    backlog: VecDeque<ActorMessage>,
    before_name: HashSet<String>,
    after_name: HashSet<String>,
    metrics: AgentMetrics,
    agent: Option<Box<ConversationalAgent>>,
}

struct AgentActor {}

impl AgentActor {
    fn start_next_actors(&self, actor_name: &str, result: &str, after_name: &HashSet<String>) {
        for next_actor in after_name {
            let res = registry::where_is(next_actor.into());
            if let Some(actor_ref) = res {
                log::info!(
                    "actor {} invoking start on actor {}",
                    actor_name,
                    actor_ref.get_name().unwrap()
                );
                actor_ref
                    .send_message(ActorMessage::StartWork(
                        actor_name.into(),
                        get_input_variables(result),
                    ))
                    .expect("Failed to send message to next actor");
            }
        }
    }

    async fn handle_internal(
        &self,
        myself: &ActorRef<ActorMessage>,
        message: ActorMessage,
        state: &mut AgentState,
    ) -> Option<ActorMessage> {
        let name = myself.get_name().unwrap();
        match &message {
            ActorMessage::StartWork(from, input) => {
                log::info!("Start Work message received for actor {} ", name);
                if !state.before_name.remove(from) {
                    log::error!("Attempting to remove {} from before_name, which is already removed for actor {} ", from, name);
                }
                if state.before_name.is_empty() {
                    log::debug!("Message from all before actors received for actor {}", name);
                    log::info!("Start processing on actor {}", name);
                } else {
                    log::debug!("Actor {}: All the before actors not completed", name);
                    return None;
                }
                if state.agent.is_some() {
                    let ag = state.agent.as_ref().unwrap().clone();
                    let ag2 = *ag;

                    let executor: AgentExecutor<ConversationalAgent> =
                        AgentExecutor::from_agent(ag2.into());

                    match executor.invoke(input.clone()).await {
                        Ok(result) => {
                            println!("Result: {:?}", result);
                        }
                        Err(e) => panic!("Error invoking LLMChain: {:?}", e),
                    }
                } else {
                    log::info!("WARNING : No agent present");
                }

                log::debug!(
                    "Processing complete for Actor {}, Initimating next actors",
                    name
                );
                self.start_next_actors(&name, "", &state.after_name);
                log::debug!("Stopping Actor {}", name);
                myself.stop(Some("pipeline completed".into()));
                None
            }
            ActorMessage::EndWork(_from, _) => {
                log::info!("End work for Actor {}", name);
                None
            }
            ActorMessage::RestartWork(_from, input) => {
                log::info!("Restarting for  Actor {}", name);
                let _ = cast(
                    myself,
                    ActorMessage::StartWork(name, get_input_variables(input)),
                );

                None
            }
            _default => {
                log::info!("Unknown message is recieved for actor {}", name);
                None
            }
        }
    }
}

fn get_input_variables(result: &str) -> HashMap<String, Value> {
    let input_variables = prompt_args! {
        "input" => result,
    };
    input_variables
}

struct ControlAgent {}

impl ControlAgent {
    fn start_next_actors(
        &self,
        actor_name: &str,
        input: HashMap<String, Value>,
        after_name: &HashSet<String>,
    ) {
        for next_actor in after_name {
            let res = registry::where_is(next_actor.into());
            if let Some(actor_ref) = res {
                log::info!(
                    "actor {} invoking start on actor {}",
                    actor_name,
                    actor_ref.get_name().unwrap()
                );
                actor_ref
                    .send_message(ActorMessage::StartWork(actor_name.into(), input.clone()))
                    .expect("Failed to send message to next actor");
            }
        }
    }
}
#[async_trait]
impl Actor for ControlAgent {
    type Msg = ActorMessage;
    type State = AgentState;
    type Arguments = AgentArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: AgentArguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        log::info!("Pre start api invoked on Control actor {}", args.agent_name);
        Ok(Self::State {
            agent_name: args.agent_name,
            owned_by: myself,
            backlog: VecDeque::new(),
            completed: false,
            before_name: args.before_name,
            after_name: args.after_name,
            metrics: AgentMetrics {
                state_change_count: 0,
            },
            agent: None,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let name = myself.get_name().unwrap_or("no name".to_string());
        log::debug!("Control actor {} handle invoked", name);
        if &state.agent_name == START_NODE {
            match &message {
                ActorMessage::StartWork(_, input) => {
                    log::info!("Start Work message received for control actor {}", name);
                    self.start_next_actors(&name, input.clone(), &state.after_name);
                    log::debug!("Stopping Actor start");
                    myself.stop(None);
                }
                _default => {
                    log::info!(
                        "Unknown message is received by actor {}, ignoring it...",
                        name
                    );
                }
            }
        } else if &state.agent_name == END_NODE {
            match &message {
                ActorMessage::StartWork(from, result) => {
                    log::info!("Start Work message received for control actor {}", name);
                    if !state.before_name.remove(from) {
                        log::error!("Attempting to remove {} from before_name, which is already removed for actor {} ", from, name);
                    }
                    log::info!("Result of agent system is {:?}", result);
                    if state.before_name.is_empty() {
                        log::debug!("Stopping Actor {}", name);
                        myself.stop(Some("pipeline completed".into()));
                    }
                }
                _default => {
                    log::info!(
                        "Unknown message is received by actor {}, ignoring it...",
                        name
                    );
                }
            }
        } else {
            panic!("Invalid agent name");
        }
        Ok(())
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
        log::info!("Pre start api invoked on actor {}", args.agent_name);
        Ok(Self::State {
            agent_name: args.agent_name,
            owned_by: myself,
            backlog: VecDeque::new(),
            completed: false,
            before_name: args.before_name,
            after_name: args.after_name,
            metrics: AgentMetrics {
                state_change_count: 0,
            },
            agent: None,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        log::info!(
            "handle invoked on actor {}",
            myself.get_name().unwrap_or("no name".to_string())
        );
        let mut maybe_unhandled = self.handle_internal(&myself, message, state).await;
        if let Some(message) = maybe_unhandled {
            state.backlog.push_back(message);
        } else {
            // we handled the message, check the queue for any work to dequeue and handle
            while !state.backlog.is_empty() && maybe_unhandled.is_none() {
                let head = state.backlog.pop_front().unwrap();
                maybe_unhandled = self.handle_internal(&myself, head, state).await;
            }
            // put the first unhandled msg back to the front of the queue
            if let Some(msg) = maybe_unhandled {
                state.backlog.push_front(msg);
            }
        }
        Ok(())
    }
}

struct AgentSystem {
    nodes_after: HashMap<String, HashSet<String>>,
    nodes_before: HashMap<String, HashSet<String>>,
    agent_map: HashMap<String, Box<dyn Agent>>,
    actors: HashMap<String, ActorRef<ActorMessage>>,
    all_handles: tokio::task::JoinSet<Result<(), JoinError>>,
}

const START_NODE: &str = "start";
const END_NODE: &str = "end";

impl AgentSystem {
    fn new() -> Self {
        let mut nodes_after: HashMap<String, HashSet<String>> = HashMap::new();
        nodes_after.insert(START_NODE.into(), HashSet::new());

        let mut nodes_before: HashMap<String, HashSet<String>> = HashMap::new();
        nodes_before.insert(END_NODE.into(), HashSet::new());
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

    fn add_next_agent(
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
        self
    }

    // Spawn all the actors and trigger start
    async fn start(&mut self, input: HashMap<String, Value>) -> &mut Self {
        for (agent_name, agent) in self.agent_map.drain() {
            let (actor, handle) = Actor::spawn(
                Some(agent_name.clone()),
                AgentActor {},
                AgentArguments {
                    after_name: self.nodes_after.get(&agent_name).unwrap().clone(),
                    before_name: self.nodes_before.get(&agent_name).unwrap().clone(),
                    agent: Some(agent),
                    agent_name: agent_name.clone(),
                },
            )
            .await
            .expect("failed to create agent actor");
            self.actors.insert(agent_name, actor);
            self.all_handles.spawn(handle);
        }
        // Add start and end control actors
        let (actor, handle) = Actor::spawn(
            Some(START_NODE.into()),
            ControlAgent {},
            AgentArguments {
                after_name: self.nodes_after.get(START_NODE).unwrap().clone(),
                before_name: HashSet::new(),
                agent: None,
                agent_name: START_NODE.into(),
            },
        )
        .await
        .expect("failed to create agent actor");
        self.actors.insert(START_NODE.into(), actor);
        self.all_handles.spawn(handle);

        let (actor, handle) = Actor::spawn(
            Some(END_NODE.into()),
            ControlAgent {},
            AgentArguments {
                after_name: self.nodes_after.get(END_NODE).unwrap().clone(),
                before_name: self.nodes_before.get(END_NODE).unwrap().clone(),
                agent: None,
                agent_name: END_NODE.into(),
            },
        )
        .await
        .expect("failed to create agent actor");
        self.actors.insert(END_NODE.into(), actor);
        self.all_handles.spawn(handle);

        let t = self.actors.get(START_NODE).unwrap();
        cast!(t, ActorMessage::StartWork(START_NODE.into(), input.into()))
            .expect("Failed to trigger the Control Start node");
        self
    }

    // Wait until all the actors have completed
    async fn wait(&mut self) {
        while self.all_handles.join_next().await.is_some() {}
    }

    // Pending tasks in the agentSystem.
    fn pending_count(&self) -> usize {
        self.all_handles.len()
    }

    // helper method to add entry.
    fn add_entry(&mut self, parent: &str, child: &str) {
        // add entry for child
        self.nodes_after.insert(child.into(), HashSet::new());

        self.nodes_before
            .entry(child.into())
            .or_insert_with(|| HashSet::new())
            .insert(parent.into());

        // add entry for parent
        self.nodes_after.entry(parent.into()).and_modify(|v| {
            v.insert(child.into());
        });
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
    use simple_logger::SimpleLogger;
    use std::env;
    use std::time::Duration;
    use std::{error::Error, sync::Arc};

    use ractor::registry;
    use serde_json::Value;

    use crate::llm::ollama::client::Ollama;
    use crate::llm::tupleleapai::client::Tupleleap;
    use crate::{
        agent::ConversationalAgentBuilder, memory::SimpleMemory, prompt_args, tools::Tool,
    };
    use leap_connect::v1::api::Client as leap_client;

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
        let _memory = SimpleMemory::new();
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
            .add_next_agent("A", "B", create_agent())
            .add_next_agent("A", "C", create_agent())
            .add_next_agent("B", "D", create_agent())
            .add_next_agent("C", "D", create_agent())
            .build();

        let mut exp_nodes_after: HashMap<String, HashSet<String>> = HashMap::new();
        exp_nodes_after.insert("D".into(), HashSet::from([END_NODE.into()]));
        exp_nodes_after.insert("A".into(), HashSet::from(["B".into(), "C".into()]));
        exp_nodes_after.insert("C".into(), HashSet::from(["D".into()]));
        exp_nodes_after.insert(END_NODE.into(), HashSet::from([]));
        exp_nodes_after.insert("B".into(), HashSet::from(["D".into()]));
        exp_nodes_after.insert(START_NODE.into(), HashSet::from(["A".into()]));

        let mut exp_nodes_before: HashMap<String, HashSet<String>> = HashMap::new();
        exp_nodes_before.insert(END_NODE.into(), HashSet::from(["D".into()]));
        exp_nodes_before.insert("D".into(), HashSet::from(["B".into(), "C".into()]));
        exp_nodes_before.insert("C".into(), HashSet::from(["A".into()]));
        exp_nodes_before.insert("A".into(), HashSet::from([START_NODE.into()]));
        exp_nodes_before.insert("B".into(), HashSet::from(["A".into()]));

        assert_eq!(agent_system.nodes_after, exp_nodes_after);
        assert_eq!(agent_system.nodes_before, exp_nodes_before);
        println!(
            "{:?}, {:?}",
            agent_system.nodes_after, agent_system.nodes_before
        );
    }

    #[tokio::test]
    async fn test_agent_system_start_single() {
        //  SimpleLogger::new().init().unwrap();

        // let llm = Ollama::default().with_model("llama3");
        let client = leap_client::new(env::var("TUPLELEAP_AI_API_KEY").unwrap().to_string());
        let llm = Tupleleap::new(client, "mistral".into());
        // memory is not required for this test.
        // let memory = SimpleMemory::new();
        let tool_calc = Calc {};
        let agent: crate::agent::ConversationalAgent = ConversationalAgentBuilder::new()
            .tools(&[Arc::new(tool_calc)])
            .build(llm)
            .unwrap();
        // let input_variables = prompt_args! {
        //     "input" => "hola,Me llamo luis, y tengo 10 anos, y estudio Computer scinence",
        // };

        let mut wrapper = AgentSystem::new();
        let agent_system: &mut AgentSystem = wrapper
            .add_agent("A", Box::new(agent))
            .build()
            .start(prompt_args! {
                "input" => "Hi from Bangalore, And what date is today",
            })
            .await;
        println!("Pending tasks {}", agent_system.pending_count());
        agent_system.wait().await;
        assert_eq!(0, agent_system.pending_count());
    }

    #[tokio::test]
    async fn test_agent_system_start_multiple() {
        SimpleLogger::new().init().unwrap();

        let mut agent_system = AgentSystem::new();
        agent_system
            .add_agent("A", create_agent())
            .add_next_agent("A", "B", create_agent())
            .add_next_agent("A", "C", create_agent())
            .add_next_agent("B", "D", create_agent())
            .add_next_agent("C", "D", create_agent())
            .build()
            .start(prompt_args! {
                "input" => "Hi from Bangalore, what is happening today?", // prompt...
            })
            .await;
        // API for human intervention// Tupelead
        // agent_system.
        println!("Pending tasks {}", agent_system.pending_count());
        agent_system.wait().await;
        assert_eq!(0, agent_system.pending_count());
    }

    #[tokio::test]
    async fn test_agent_integration() {
        SimpleLogger::new().init().unwrap();
        let run_time = Duration::from_secs(5);

        let llm = Ollama::default().with_model("llama3");
        let _memory = SimpleMemory::new();
        let tool_calc = Calc {};
        let agent: crate::agent::ConversationalAgent = ConversationalAgentBuilder::new()
            .tools(&[Arc::new(tool_calc)])
            .build(llm)
            .unwrap();
        let _input_variables = prompt_args! {
            "input" => "hola,Me llamo luis, y tengo 10 anos, y estudio Computer scinence",
        };

        let (actor, _handle) = Actor::spawn(
            Some("agentA".into()),
            AgentActor {},
            AgentArguments {
                after_name: HashSet::new(),
                before_name: HashSet::new(),
                agent: Some(Box::new(agent)),
                agent_name: "agentA".into(),
            },
        )
        .await
        .expect("failed to create agent actor");
        let r = registry::where_is("agentA".into());
        if let Some(a_ref) = r {
            // let t = cast!(a_ref, ActorMessage::StartWork(a_ref.get_id()));
            let _ = a_ref.send_message(ActorMessage::StartWork(
                "test".into(),
                prompt_args! {
                    "input" => "what date is today?",
                },
            ));
            let _ = a_ref.send_message(ActorMessage::EndWork(
                "test".into(),
                "It is a Sunday, july 14th".into(),
            ));
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
                    after_name: HashSet::new(),
                    before_name: HashSet::new(),
                    agent: Some(agent_box),
                    agent_name: actor_names[i].into(),
                },
            )
            .await
            .expect("failed to create agent actor");
            actors.push(actor);
            all_handles.spawn(handle);
        }
        let first_actor = actors[0].clone();
        let t = cast!(
            &first_actor,
            ActorMessage::StartWork("A".into(), HashMap::new())
        );
        println!("**** **** **** *** ** *");
        println!("{:?}", t);
        // wait for the simulation to end
        tokio::time::sleep(run_time).await;
        for actor in actors {
            actor.stop(None)
        }
    }
}
