use borsh::BorshSerialize;
use bytes::Bytes;
use casper_storage::global_state::{
    self,
    state::{lmdb::LmdbGlobalState, CommitProvider, StateProvider},
};
use casper_types::{
    execution::{Effects, TransformKind},
    Digest, EntityAddr, Key,
};
use digest::consts::U32;
use vm::{
    storage::Address, ConfigBuilder, ExecuteRequest, ExecuteRequestBuilder, ExecuteResult,
    ExecutionKind, Executor, ExecutorConfigBuilder, ExecutorKind, ExecutorV2, WasmEngine,
};

// use super::*;
const VM2_TEST_CONTRACT: Bytes = Bytes::from_static(include_bytes!("../vm2-test-contract.wasm"));
const VM2_HARNESS: Bytes = Bytes::from_static(include_bytes!("../vm2-harness.wasm"));
const VM2_CEP18: Bytes = Bytes::from_static(include_bytes!("../vm2-cep18.wasm"));
const VM2_CEP18_CALLER: Bytes = Bytes::from_static(include_bytes!("../vm2-cep18-caller.wasm"));
const VM2_TRAITS: Bytes = Bytes::from_static(include_bytes!("../vm2-trait.wasm"));

#[test]
fn test_contract() {
    let mut executor = make_executor();

    let (mut global_state, state_root_hash, _tempdir) =
        global_state::state::lmdb::make_temporary_global_state([]);

    let _effects = run_wasm(
        &mut executor,
        &mut global_state,
        state_root_hash,
        VM2_TEST_CONTRACT,
        ("Hello, world!".to_string(), 123456789u32),
    );
}

#[test]
fn harness() {
    let mut executor = make_executor();

    let (mut global_state, state_root_hash, _tempdir) =
        global_state::state::lmdb::make_temporary_global_state([]);
    run_wasm(
        &mut executor,
        &mut global_state,
        state_root_hash,
        VM2_HARNESS,
        (),
    );
}

fn make_executor() -> ExecutorV2 {
    let executor_config = ExecutorConfigBuilder::default()
        .with_memory_limit(17)
        .with_executor_kind(ExecutorKind::Compiled)
        .build()
        .expect("Should build");
    ExecutorV2::new(executor_config)
}

#[test]
fn cep18() {
    let mut executor = make_executor();

    let (mut global_state, mut state_root_hash, _tempdir) =
        global_state::state::lmdb::make_temporary_global_state([]);

    let effects_1 = run_wasm(
        &mut executor,
        &mut global_state,
        state_root_hash,
        VM2_CEP18,
        (),
    );

    let contract_hash = {
        let mut values: Vec<_> = effects_1
            .transforms()
            .iter()
            .filter(|t| t.key().is_smart_contract_key() && t.kind() != &TransformKind::Identity)
            .collect();
        assert_eq!(values.len(), 1, "{values:#?}");
        let transform = values.remove(0);
        let Key::AddressableEntity(EntityAddr::SmartContract(contract_hash)) = transform.key()
        else {
            panic!("Expected a smart contract key")
        };
        *contract_hash
    };

    state_root_hash = global_state
        .commit(state_root_hash, effects_1)
        .expect("Should commit");

    let _effects_2 = run_wasm(
        &mut executor,
        &mut global_state,
        state_root_hash,
        VM2_CEP18_CALLER,
        (contract_hash,),
    );
}

#[test]
fn traits() {
    let mut executor = make_executor();
    let (mut global_state, root_hash, _tempdir) =
        global_state::state::lmdb::make_temporary_global_state([]);
    run_wasm(&mut executor, &mut global_state, root_hash, VM2_TRAITS, ());
}

const ALICE: [u8; 32] = [100; 32];
const BOB: [u8; 32] = [101; 32];
const CSPR: u64 = 10u64.pow(9);

fn run_wasm<T: BorshSerialize>(
    executor: &mut ExecutorV2,
    global_state: &mut LmdbGlobalState,
    pre_state_hash: Digest,
    module_bytes: Bytes,
    input_data: T,
) -> Effects {
    {
        // "Genesis"
        // storage.update_balance(&ALICE, 10 * CSPR).unwrap();
        // storage.update_balance(&BOB, 10 * CSPR).unwrap();
    }

    // let mut vm = WasmEngine::new();

    let tracking_copy = global_state
        .tracking_copy(pre_state_hash)
        .expect("Obtaining root hash succeed")
        .expect("Root hash exists");

    let input = borsh::to_vec(&input_data).map(Bytes::from).unwrap();

    let execute_request = ExecuteRequestBuilder::default()
        .with_address([42; 32])
        .with_gas_limit(1_000_000)
        .with_target(ExecutionKind::WasmBytes(module_bytes))
        .with_input(input)
        .build()
        .expect("should build");

    let result = executor
        .execute(tracking_copy, execute_request)
        .expect("Succeed");

    result.effects().clone()
}
