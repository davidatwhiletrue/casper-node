#![cfg(test)]
use std::sync::{Arc, Mutex};

use casper_node_macros::reactor;
use futures::FutureExt;
use tempfile::TempDir;
use thiserror::Error;
use tokio::time;

use super::*;
use crate::{
    components::{
        deploy_acceptor,
        in_memory_network::{NetworkController, NodeId},
        storage::{self, Storage, StorageType},
    },
    effect::announcements::{DeployAcceptorAnnouncement, NetworkAnnouncement},
    protocol::Message,
    reactor::{Reactor as ReactorTrait, Runner},
    testing::{
        network::{Network, NetworkedReactor},
        ConditionCheckReactor, TestRng,
    },
    types::{Deploy, DeployHash},
    utils::WithDir,
};

const TIMEOUT: Duration = Duration::from_secs(1);

/// Error type returned by the test reactor.
#[derive(Debug, Error)]
enum Error {
    #[error("prometheus (metrics) error: {0}")]
    Metrics(#[from] prometheus::Error),
}

impl Drop for Reactor {
    fn drop(&mut self) {
        NetworkController::<Message>::remove_node(&self.network.node_id())
    }
}

#[derive(Debug)]
pub struct FetcherTestConfig {
    gossiper_config: GossipConfig,
    storage_config: storage::Config,
    temp_dir: TempDir,
}

impl Default for FetcherTestConfig {
    fn default() -> Self {
        let (storage_config, temp_dir) = storage::Config::default_for_tests();
        FetcherTestConfig {
            gossiper_config: Default::default(),
            storage_config,
            temp_dir,
        }
    }
}

reactor!(Reactor {
    type Config = FetcherTestConfig;

    components: {
        network = infallible InMemoryNetwork::<Message>(event_queue, rng);
        storage = Storage(WithDir::new(cfg.temp_dir.path(), cfg.storage_config));
        deploy_acceptor = infallible DeployAcceptor();
        deploy_fetcher = infallible Fetcher::<Deploy>(cfg.gossiper_config);
    }

    events: {
        network = Event<Message>;
        storage = Event<Storage>;
        deploy_fetcher = Event<Deploy>;
    }

    requests: {
        // This tests contains no linear chain requests, so we panic if we receive any.
        LinearChainRequest<NodeId> -> !;
        NetworkRequest<NodeId, Message> -> network;
        StorageRequest<Storage> -> storage;
        FetcherRequest<NodeId, Deploy> -> deploy_fetcher;
    }

    announcements: {
        // The deploy fetcher needs to be notified about new deploys.
        DeployAcceptorAnnouncement<NodeId> -> [deploy_fetcher];
        // TODO: Route network messages
        NetworkAnnouncement<NodeId, Message> -> [fn handle_message];
        // Currently the ApiServerAnnouncement is misnamed - it solely tells of new deploys arriving
        // from a client.
        ApiServerAnnouncement -> [deploy_acceptor];
    }
});

impl Reactor {
    fn handle_message(
        &mut self,
        effect_builder: EffectBuilder<ReactorEvent>,
        rng: &mut dyn CryptoRngCore,
        network_announcement: NetworkAnnouncement<NodeId, Message>,
    ) -> Effects<ReactorEvent> {
        // TODO: Make this manual routing disappear and supply appropriate
        // announcements.
        match network_announcement {
            NetworkAnnouncement::MessageReceived { sender, payload } => match payload {
                Message::GetRequest { serialized_id, .. } => {
                    let deploy_hash = match bincode::deserialize(&serialized_id) {
                        Ok(hash) => hash,
                        Err(error) => {
                            error!(
                                "failed to decode {:?} from {}: {}",
                                serialized_id, sender, error
                            );
                            return Effects::new();
                        }
                    };

                    self.dispatch_event(
                        effect_builder,
                        rng,
                        ReactorEvent::Storage(storage::Event::GetDeployForPeer {
                            deploy_hash,
                            peer: sender,
                        }),
                    )
                }

                Message::GetResponse {
                    serialized_item, ..
                } => {
                    let deploy = match bincode::deserialize(&serialized_item) {
                        Ok(deploy) => Box::new(deploy),
                        Err(error) => {
                            error!("failed to decode deploy from {}: {}", sender, error);
                            return Effects::new();
                        }
                    };

                    self.dispatch_event(
                        effect_builder,
                        rng,
                        ReactorEvent::DeployAcceptor(deploy_acceptor::Event::Accept {
                            deploy,
                            source: Source::Peer(sender),
                        }),
                    )
                }
                msg => panic!("should not get {}", msg),
            },
            ann => panic!("should not received any network announcements: {:?}", ann),
        }
    }
}

impl NetworkedReactor for Reactor {
    type NodeId = NodeId;

    fn node_id(&self) -> NodeId {
        self.network.node_id()
    }
}

fn announce_deploy_received(
    deploy: Deploy,
) -> impl FnOnce(EffectBuilder<ReactorEvent>) -> Effects<ReactorEvent> {
    |effect_builder: EffectBuilder<ReactorEvent>| {
        effect_builder
            .announce_deploy_received(Box::new(deploy))
            .ignore()
    }
}

fn fetch_deploy(
    deploy_hash: DeployHash,
    node_id: NodeId,
    fetched: Arc<Mutex<(bool, Option<FetchResult<Deploy>>)>>,
) -> impl FnOnce(EffectBuilder<ReactorEvent>) -> Effects<ReactorEvent> {
    move |effect_builder: EffectBuilder<ReactorEvent>| {
        effect_builder
            .fetch_deploy(deploy_hash, node_id)
            .then(move |maybe_deploy| async move {
                let mut result = fetched.lock().unwrap();
                result.0 = true;
                result.1 = maybe_deploy;
            })
            .ignore()
    }
}

/// Store a deploy on a target node.
async fn store_deploy(
    deploy: &Deploy,
    node_id: &NodeId,
    network: &mut Network<Reactor>,
    mut rng: &mut TestRng,
) {
    network
        .process_injected_effect_on(node_id, announce_deploy_received(deploy.clone()))
        .await;

    // cycle to deploy acceptor announcement
    network
        .crank_until(
            node_id,
            &mut rng,
            move |event: &ReactorEvent| -> bool {
                match event {
                    ReactorEvent::DeployAcceptorAnnouncement(
                        DeployAcceptorAnnouncement::AcceptedNewDeploy { .. },
                    ) => true,
                    _ => false,
                }
            },
            TIMEOUT,
        )
        .await;
}

async fn assert_settled(
    node_id: &NodeId,
    deploy_hash: DeployHash,
    expected_result: Option<FetchResult<Deploy>>,
    fetched: Arc<Mutex<(bool, Option<FetchResult<Deploy>>)>>,
    network: &mut Network<Reactor>,
    rng: &mut TestRng,
    timeout: Duration,
) {
    let has_responded = |_nodes: &HashMap<NodeId, Runner<ConditionCheckReactor<Reactor>>>| {
        fetched.lock().unwrap().0
    };

    network.settle_on(rng, has_responded, timeout).await;

    let maybe_stored_deploy = network
        .nodes()
        .get(node_id)
        .unwrap()
        .reactor()
        .inner()
        .storage
        .deploy_store()
        .get(smallvec![deploy_hash])
        .pop()
        .expect("should only be a single result")
        .expect("should not error while getting");
    assert_eq!(expected_result.is_some(), maybe_stored_deploy.is_some());

    assert_eq!(fetched.lock().unwrap().1, expected_result)
}

#[tokio::test]
async fn should_fetch_from_local() {
    const NETWORK_SIZE: usize = 1;

    NetworkController::<Message>::create_active();
    let (mut network, mut rng, node_ids) = {
        let mut network = Network::<Reactor>::new();
        let mut rng = TestRng::new();
        let node_ids = network.add_nodes(&mut rng, NETWORK_SIZE).await;
        (network, rng, node_ids)
    };

    // Create a random deploy.
    let deploy = Deploy::random(&mut rng);

    // Store deploy on a node.
    let node_to_store_on = &node_ids[0];
    store_deploy(&deploy, node_to_store_on, &mut network, &mut rng).await;

    // Try to fetch the deploy from a node that holds it.
    let node_id = &node_ids[0];
    let deploy_hash = *deploy.id();
    let fetched = Arc::new(Mutex::new((false, None)));
    network
        .process_injected_effect_on(
            node_id,
            fetch_deploy(deploy_hash, *node_id, Arc::clone(&fetched)),
        )
        .await;

    let expected_result = Some(FetchResult::FromStorage(Box::new(deploy)));
    assert_settled(
        node_id,
        deploy_hash,
        expected_result,
        fetched,
        &mut network,
        &mut rng,
        TIMEOUT,
    )
    .await;

    NetworkController::<Message>::remove_active();
}

#[tokio::test]
async fn should_fetch_from_peer() {
    const NETWORK_SIZE: usize = 2;

    NetworkController::<Message>::create_active();
    let (mut network, mut rng, node_ids) = {
        let mut network = Network::<Reactor>::new();
        let mut rng = TestRng::new();
        let node_ids = network.add_nodes(&mut rng, NETWORK_SIZE).await;
        (network, rng, node_ids)
    };

    // Create a random deploy.
    let deploy = Deploy::random(&mut rng);

    // Store deploy on a node.
    let node_with_deploy = &node_ids[0];
    store_deploy(&deploy, node_with_deploy, &mut network, &mut rng).await;

    let node_without_deploy = &node_ids[1];
    let deploy_hash = *deploy.id();
    let fetched = Arc::new(Mutex::new((false, None)));

    // Try to fetch the deploy from a node that does not hold it; should get from peer.
    network
        .process_injected_effect_on(
            node_without_deploy,
            fetch_deploy(deploy_hash, *node_with_deploy, Arc::clone(&fetched)),
        )
        .await;

    let expected_result = Some(FetchResult::FromPeer(Box::new(deploy), *node_with_deploy));
    assert_settled(
        node_without_deploy,
        deploy_hash,
        expected_result,
        fetched,
        &mut network,
        &mut rng,
        TIMEOUT,
    )
    .await;

    NetworkController::<Message>::remove_active();
}

#[tokio::test]
async fn should_timeout_fetch_from_peer() {
    const NETWORK_SIZE: usize = 2;

    NetworkController::<Message>::create_active();
    let (mut network, mut rng, node_ids) = {
        let mut network = Network::<Reactor>::new();
        let mut rng = TestRng::new();
        let node_ids = network.add_nodes(&mut rng, NETWORK_SIZE).await;
        (network, rng, node_ids)
    };

    // Create a random deploy.
    let deploy = Deploy::random(&mut rng);
    let deploy_hash = *deploy.id();

    let holding_node = node_ids[0];
    let requesting_node = node_ids[1];

    // Store deploy on holding node.
    store_deploy(&deploy, &holding_node, &mut network, &mut rng).await;

    // Initiate requesting node asking for deploy from holding node.
    let fetched = Arc::new(Mutex::new((false, None)));
    network
        .process_injected_effect_on(
            &requesting_node,
            fetch_deploy(deploy_hash, holding_node, Arc::clone(&fetched)),
        )
        .await;

    // Crank until message sent from the requester.
    network
        .crank_until(
            &requesting_node,
            &mut rng,
            move |event: &ReactorEvent| -> bool {
                if let ReactorEvent::NetworkRequest(NetworkRequest::SendMessage {
                    payload: Message::GetRequest { .. },
                    ..
                }) = event
                {
                    true
                } else {
                    false
                }
            },
            TIMEOUT,
        )
        .await;

    // Crank until the message is received by the holding node.
    network
        .crank_until(
            &holding_node,
            &mut rng,
            move |event: &ReactorEvent| -> bool {
                if let ReactorEvent::NetworkRequest(NetworkRequest::SendMessage {
                    payload: Message::GetResponse { .. },
                    ..
                }) = event
                {
                    true
                } else {
                    false
                }
            },
            TIMEOUT,
        )
        .await;

    // Advance time.
    let secs_to_advance = GossipConfig::default().get_remainder_timeout_secs();
    time::pause();
    time::advance(Duration::from_secs(secs_to_advance + 10)).await;
    time::resume();

    // Settle the network, allowing timeout to avoid panic.
    let expected_result = None;
    assert_settled(
        &requesting_node,
        deploy_hash,
        expected_result,
        fetched,
        &mut network,
        &mut rng,
        TIMEOUT,
    )
    .await;

    NetworkController::<Message>::remove_active();
}
