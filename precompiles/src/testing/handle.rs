// This file is part of Frontier.

// Copyright (c) Moonsong Labs.
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::testing::PrettyLog;
use alloc::boxed::Box;
use evm::{ExitRevert, ExitSucceed};
use fp_evm::{Context, ExitError, ExitReason, Log, PrecompileHandle, Transfer};
use sp_core::{H160, H256};

use super::Alice;

#[derive(Debug, Clone)]
pub struct Subcall {
	pub address: H160,
	pub transfer: Option<Transfer>,
	pub input: Vec<u8>,
	pub target_gas: Option<u64>,
	pub is_static: bool,
	pub context: Context,
}

#[derive(Debug, Clone)]
pub struct SubcallOutput {
	pub reason: ExitReason,
	pub output: Vec<u8>,
	pub cost: u64,
	pub logs: Vec<Log>,
}

impl SubcallOutput {
	pub fn revert() -> Self {
		Self {
			reason: ExitReason::Revert(ExitRevert::Reverted),
			output: Vec::new(),
			cost: 0,
			logs: Vec::new(),
		}
	}

	pub fn succeed() -> Self {
		Self {
			reason: ExitReason::Succeed(ExitSucceed::Returned),
			output: Vec::new(),
			cost: 0,
			logs: Vec::new(),
		}
	}

	pub fn out_of_gas() -> Self {
		Self {
			reason: ExitReason::Error(ExitError::OutOfGas),
			output: Vec::new(),
			cost: 0,
			logs: Vec::new(),
		}
	}
}

pub trait SubcallTrait: FnMut(Subcall) -> SubcallOutput + 'static {}

impl<T: FnMut(Subcall) -> SubcallOutput + 'static> SubcallTrait for T {}

pub type SubcallHandle = Box<dyn SubcallTrait>;

/// Mock handle to write tests for precompiles.
pub struct MockHandle {
	pub gas_limit: u64,
	pub gas_used: u64,
	pub logs: Vec<PrettyLog>,
	pub subcall_handle: Option<SubcallHandle>,
	pub code_address: H160,
	pub input: Vec<u8>,
	pub context: Context,
	pub is_static: bool,
}

impl MockHandle {
	pub fn new(code_address: H160, context: Context) -> Self {
		Self {
			gas_limit: u64::MAX,
			gas_used: 0,
			logs: vec![],
			subcall_handle: None,
			code_address,
			input: Vec::new(),
			context,
			is_static: false,
		}
	}
}

impl PrecompileHandle for MockHandle {
	/// Perform subcall in provided context.
	/// Precompile specifies in which context the subcall is executed.
	fn call(
		&mut self,
		address: H160,
		transfer: Option<Transfer>,
		input: Vec<u8>,
		target_gas: Option<u64>,
		is_static: bool,
		context: &Context,
	) -> (ExitReason, Vec<u8>) {
		if self
			.record_cost(crate::evm::costs::call_cost(
				context.apparent_value,
				&evm::Config::pectra(),
			))
			.is_err()
		{
			return (ExitReason::Error(ExitError::OutOfGas), vec![]);
		}

		match &mut self.subcall_handle {
			Some(handle) => {
				let SubcallOutput {
					reason,
					output,
					cost,
					logs,
				} = handle(Subcall {
					address,
					transfer,
					input,
					target_gas,
					is_static,
					context: context.clone(),
				});

				if self.record_cost(cost).is_err() {
					return (ExitReason::Error(ExitError::OutOfGas), vec![]);
				}

				for log in logs {
					self.log(log.address, log.topics, log.data)
						.expect("cannot fail");
				}

				(reason, output)
			}
			None => panic!("no subcall handle registered"),
		}
	}

	fn record_cost(&mut self, cost: u64) -> Result<(), ExitError> {
		self.gas_used += cost;

		if self.gas_used > self.gas_limit {
			Err(ExitError::OutOfGas)
		} else {
			Ok(())
		}
	}

	fn remaining_gas(&self) -> u64 {
		self.gas_limit - self.gas_used
	}

	fn log(&mut self, address: H160, topics: Vec<H256>, data: Vec<u8>) -> Result<(), ExitError> {
		self.logs.push(PrettyLog(Log {
			address,
			topics,
			data,
		}));
		Ok(())
	}

	/// Retrieve the code address (what is the address of the precompile being called).
	fn code_address(&self) -> H160 {
		self.code_address
	}

	/// Retrieve the input data the precompile is called with.
	fn input(&self) -> &[u8] {
		&self.input
	}

	/// Retrieve the context in which the precompile is executed.
	fn context(&self) -> &Context {
		&self.context
	}

	/// Is the precompile call is done statically.
	fn is_static(&self) -> bool {
		self.is_static
	}

	/// Retrieve the gas limit of this call.
	fn gas_limit(&self) -> Option<u64> {
		Some(self.gas_limit)
	}

	fn record_external_cost(
		&mut self,
		_ref_time: Option<u64>,
		_proof_size: Option<u64>,
		_storage_growth: Option<u64>,
	) -> Result<(), ExitError> {
		Ok(())
	}

	fn refund_external_cost(&mut self, _ref_time: Option<u64>, _proof_size: Option<u64>) {}

	fn origin(&self) -> H160 {
		Alice.into()
	}

	fn is_contract_being_constructed(&self, _address: H160) -> bool {
		false
	}
}
