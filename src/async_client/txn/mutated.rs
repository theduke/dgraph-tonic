use crate::async_client::txn::default::Base;
use crate::async_client::txn::{IState, TxnState, TxnVariant};
use crate::async_client::IDgraphClient;
use crate::errors::DgraphError;
use crate::{Assigned, Mutation, Request};
use async_trait::async_trait;
use log::error;
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct Mutated {
    base: Base,
    mutated: bool,
}

#[async_trait]
impl IState for Mutated {
    async fn commit_or_abort(&self, state: TxnState) -> Result<(), DgraphError> {
        if !self.mutated {
            return Ok(());
        }
        let mut client = state.client;
        let txn = state.context;
        match client.commit_or_abort(txn).await {
            Ok(_txn_context) => Ok(()),
            Err(err) => {
                error!("Cannot commit mutated transaction. err: {:?}", err);
                Err(DgraphError::GrpcError)
            }
        }
    }

    fn query_request(
        &self,
        state: &TxnState,
        query: String,
        vars: HashMap<String, String, RandomState>,
    ) -> Request {
        self.base.query_request(state, query, vars)
    }
}

impl TxnVariant<Base> {
    pub fn mutated(self) -> TxnVariant<Mutated> {
        TxnVariant {
            state: self.state,
            extra: Mutated {
                base: self.extra,
                mutated: false,
            },
        }
    }
}

impl TxnVariant<Mutated> {
    async fn do_mutation(&mut self, mut mu: Mutation) -> Result<Assigned, DgraphError> {
        self.extra.mutated = true;
        mu.start_ts = self.context.start_ts;
        let assigned = match self.client.mutate(mu).await {
            Ok(assigned) => assigned,
            Err(err) => {
                error!("Cannot mutate transaction. err: {:?}", err);
                return Err(DgraphError::GrpcError);
            }
        };
        match assigned.context.as_ref() {
            Some(src) => self.context.merge_context(src)?,
            None => return Err(DgraphError::MissingTxnContext),
        }
        Ok(assigned)
    }

    pub async fn mutate(&mut self, mut mu: Mutation) -> Result<Assigned, DgraphError> {
        mu.commit_now = false;
        self.do_mutation(mu).await
    }

    pub async fn mutate_and_commit_now(
        mut self,
        mut mu: Mutation,
    ) -> Result<Assigned, DgraphError> {
        mu.commit_now = true;
        self.do_mutation(mu).await
    }

    pub async fn commit(self) -> Result<(), DgraphError> {
        self.commit_or_abort().await
    }
}
