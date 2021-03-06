use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::net::SocketAddr;

use crate::error::Error;

use super::rpc;

pub struct BoostrapController {
    state: Cell<BootstrapState>,
    local_id: u64,
    peers: RefCell<BTreeMap<u64, Vec<SocketAddr>>>,
    responded_ids: RefCell<HashSet<u64>>,
    pending_actions_buffer: RefCell<VecDeque<BootstrapAction>>,
}

#[derive(Debug, Copy, Clone)]
enum BootstrapState {
    Cold,
    Warm,
    Offline,
    Done,
}

pub enum BootstrapEvent {
    Request(rpc::BootstrapMsg),
    Response(rpc::BootstrapMsg),
    Timeout,
}

#[derive(Debug)]
pub enum BootstrapAction {
    Request(rpc::BootstrapMsg, Vec<SocketAddr>),
    Response(Result<rpc::Response, Error>),
    Completed,
}

impl BoostrapController {
    pub fn new(
        local_id: u64,
        local_addrs: Vec<SocketAddr>,
        bootstrap_addrs: Vec<Vec<SocketAddr>>,
    ) -> Self {
        let mut peers = BTreeMap::new();
        peers.insert(local_id, local_addrs);

        let bootstrap_controller = BoostrapController {
            state: Cell::new(BootstrapState::Cold),
            local_id,
            peers: RefCell::new(peers),
            responded_ids: Default::default(),
            pending_actions_buffer: Default::default(),
        };
        bootstrap_controller.broadcast(bootstrap_addrs.into_iter());
        bootstrap_controller
    }

    pub fn pending_actions(&self) -> Vec<BootstrapAction> {
        self.pending_actions_buffer
            .borrow_mut()
            .drain(..)
            .collect::<Vec<BootstrapAction>>()
    }

    pub fn handle_event(&self, event: BootstrapEvent) {
        use BootstrapEvent as E;
        use BootstrapState as S;

        let new_state = match (self.state.get(), event) {
            (S::Cold, E::Request(req))
            | (S::Cold, E::Response(req))
            | (S::Offline, E::Request(req)) => {
                self.handle_msg(req);
                Some(S::Warm)
            }
            (S::Warm, E::Request(req)) | (S::Warm, E::Response(req)) => {
                self.handle_msg(req);

                let num_peers = self.peers.borrow().len();
                let num_responded = self.responded_ids.borrow().len();
                if num_peers == (num_responded + 1) {
                    self.send(BootstrapAction::Completed);
                    Some(S::Done)
                } else {
                    None
                }
            }
            (S::Cold, E::Timeout) => Some(S::Offline),
            (S::Offline, E::Timeout) => None,
            _ => panic!("invalid state"),
        };

        if let Some(new_state) = new_state {
            self.state.set(new_state);
        }
    }

    fn handle_msg(&self, req: rpc::BootstrapMsg) {
        if req.from_id == self.local_id {
            return;
        }

        let mut responded_ids = self.responded_ids.borrow_mut();
        if !responded_ids.contains(&req.from_id) {
            let new_nodes = self.merge_nodes_list(&req.nodes);
            let new_nodes = new_nodes.iter().map(|(_, addrs)| addrs.clone());
            self.broadcast(new_nodes);

            responded_ids.insert(req.from_id);
        }
    }

    #[inline]
    fn broadcast(&self, addrs: impl Iterator<Item = Vec<SocketAddr>>) {
        for peer_addrs in addrs {
            self.send_request(peer_addrs.clone());
        }
    }

    #[inline]
    fn send_request(&self, to: Vec<SocketAddr>) {
        let nodes = self
            .peers
            .borrow()
            .iter()
            .map(|(a, b)| (*a, b.clone()))
            .collect();

        let peers = self.peers.borrow();
        let local_addrs = peers.get(&self.local_id).unwrap();
        self.send(BootstrapAction::Request(
            rpc::BootstrapMsg {
                from_id: self.local_id,
                from_addrs: local_addrs.clone(),
                nodes,
            },
            to,
        ));
    }

    #[inline]
    fn send(&self, action: BootstrapAction) {
        self.pending_actions_buffer.borrow_mut().push_back(action)
    }

    /// Merges `other` nodes list to already known. Returns new nodes count
    fn merge_nodes_list(&self, other: &Vec<(u64, Vec<SocketAddr>)>) -> Vec<(u64, Vec<SocketAddr>)> {
        let mut new_nodes = Vec::<(u64, Vec<SocketAddr>)>::with_capacity(other.len());
        {
            let self_nodes = self.peers.borrow();
            // a - already known nodes
            // b - received from peer nodes list
            let mut a_iter = self_nodes.iter();
            let mut b_iter = other.into_iter();
            let mut a = a_iter.next();
            let mut b = b_iter.next();

            while let (Some((a_id, _)), Some((b_id, b_addr))) = (a, b) {
                let a_id = *a_id;
                let b_id = *b_id;
                if b_id < a_id {
                    new_nodes.push((b_id, b_addr.clone()));
                    b = b_iter.next();
                } else if b_id > a_id {
                    a = a_iter.next();
                } else {
                    a = a_iter.next();
                    b = b_iter.next();
                }
            }

            while let Some((id, addr)) = b {
                new_nodes.push((*id, addr.clone()));
                b = b_iter.next();
            }
        }

        let mut known_nodes = self.peers.borrow_mut();
        for (id, addr) in new_nodes.iter() {
            known_nodes.insert(*id, addr.clone());
        }
        new_nodes
    }
}
