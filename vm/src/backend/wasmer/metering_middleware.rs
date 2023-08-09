use std::sync::Arc;

use wasmer::{wasmparser::Operator, ModuleMiddleware};
use wasmer_middlewares::Metering;

// use casper_types::shared::OpcodeCosts;

// #[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
// pub enum InstructionType {
//     Bit,
//     Add,
//     Mul,
//     Div,
//     Load,
//     Store,
//     Const,
//     FloatConst,
//     Local,
//     Global,
//     ControlFlow,
//     IntegerComparison,
//     FloatComparison,
//     Float,
//     Conversion,
//     FloatConversion,
//     Reinterpretation,
//     Unreachable,
//     Nop,
//     CurrentMemory,
//     GrowMemory(u32),
// }

// fn cost_function(opcode_costs: OpcodeCosts, operator: &Operator) -> u64 {
// let instruction_type = match operator {
//     Operator::Unreachable => InstructionType::Unreachable,
//     Operator::Nop => InstructionType::Nop,
//     Operator::Block { .. } => InstructionType::ControlFlow,
//     Operator::Loop { .. } => InstructionType::ControlFlow,
//     Operator::If { .. } => InstructionType::ControlFlow,
//     Operator::Else => InstructionType::ControlFlow,
//     Operator::End => InstructionType::ControlFlow,
//     Operator::Br { .. } => InstructionType::ControlFlow,
//     Operator::BrIf { .. } => InstructionType::ControlFlow,
//     Operator::BrTable { .. } => InstructionType::ControlFlow,
//     Operator::Return => InstructionType::ControlFlow,
//     Operator::Call { .. } => InstructionType::ControlFlow,
//     Operator::CallIndirect { .. } => InstructionType::ControlFlow,
//     Operator::Drop => InstructionType::ControlFlow,
//     Operator::Select => InstructionType::ControlFlow,

//     Operator::LocalGet { .. } => InstructionType::Local,
//     Operator::LocalSet { .. } => InstructionType::Local,
//     Operator::LocalTee { .. } => InstructionType::Local,
//     Operator::GlobalGet { .. } => InstructionType::Global,
//     Operator::GlobalSet { .. } => InstructionType::Global,

//     Operator::I32Load { .. } => InstructionType::Load,
//     Operator::I64Load { .. } => InstructionType::Load,
//     Operator::F32Load { .. } => InstructionType::Load,
//     Operator::F64Load { .. } => InstructionType::Load,
//     Operator::I32Load8S { .. } => InstructionType::Load,
//     Operator::I32Load8U { .. } => InstructionType::Load,
//     Operator::I32Load16S { .. } => InstructionType::Load,
//     Operator::I32Load16U { .. } => InstructionType::Load,
//     Operator::I64Load8S { .. } => InstructionType::Load,
//     Operator::I64Load8U { .. } => InstructionType::Load,
//     Operator::I64Load16S { .. } => InstructionType::Load,
//     Operator::I64Load16U { .. } => InstructionType::Load,
//     Operator::I64Load32S { .. } => InstructionType::Load,
//     Operator::I64Load32U { .. } => InstructionType::Load,

//     Operator::I32Store { .. } => InstructionType::Store,
//     Operator::I64Store { .. } => InstructionType::Store,
//     Operator::F32Store { .. } => InstructionType::Store,
//     Operator::F64Store { .. } => InstructionType::Store,
//     Operator::I32Store8 { .. } => InstructionType::Store,
//     Operator::I32Store16 { .. } => InstructionType::Store,
//     Operator::I64Store8 { .. } => InstructionType::Store,
//     Operator::I64Store16 { .. } => InstructionType::Store,
//     Operator::I64Store32 { .. } => InstructionType::Store,

//     Operator::MemorySize { .. } => InstructionType::CurrentMemory,
//     Operator::MemoryGrow { mem, mem_byte } => InstructionType::GrowMemory((*mem_byte).into()),

//     Operator::I32Const { .. } => InstructionType::Const,
//     Operator::I64Const { .. } => InstructionType::Const,

//     Operator::F32Const { .. } => InstructionType::FloatConst,
//     Operator::F64Const { .. } => InstructionType::FloatConst,

//     Operator::I32Eqz => InstructionType::IntegerComparison,
//     Operator::I32Eq => InstructionType::IntegerComparison,
//     Operator::I32Ne => InstructionType::IntegerComparison,
//     Operator::I32LtS => InstructionType::IntegerComparison,
//     Operator::I32LtU => InstructionType::IntegerComparison,
//     Operator::I32GtS => InstructionType::IntegerComparison,
//     Operator::I32GtU => InstructionType::IntegerComparison,
//     Operator::I32LeS => InstructionType::IntegerComparison,
//     Operator::I32LeU => InstructionType::IntegerComparison,
//     Operator::I32GeS => InstructionType::IntegerComparison,
//     Operator::I32GeU => InstructionType::IntegerComparison,

//     Operator::I64Eqz => InstructionType::IntegerComparison,
//     Operator::I64Eq => InstructionType::IntegerComparison,
//     Operator::I64Ne => InstructionType::IntegerComparison,
//     Operator::I64LtS => InstructionType::IntegerComparison,
//     Operator::I64LtU => InstructionType::IntegerComparison,
//     Operator::I64GtS => InstructionType::IntegerComparison,
//     Operator::I64GtU => InstructionType::IntegerComparison,
//     Operator::I64LeS => InstructionType::IntegerComparison,
//     Operator::I64LeU => InstructionType::IntegerComparison,
//     Operator::I64GeS => InstructionType::IntegerComparison,
//     Operator::I64GeU => InstructionType::IntegerComparison,

//     Operator::F32Eq => InstructionType::FloatComparison,
//     Operator::F32Ne => InstructionType::FloatComparison,
//     Operator::F32Lt => InstructionType::FloatComparison,
//     Operator::F32Gt => InstructionType::FloatComparison,
//     Operator::F32Le => InstructionType::FloatComparison,
//     Operator::F32Ge => InstructionType::FloatComparison,

//     Operator::F64Eq => InstructionType::FloatComparison,
//     Operator::F64Ne => InstructionType::FloatComparison,
//     Operator::F64Lt => InstructionType::FloatComparison,
//     Operator::F64Gt => InstructionType::FloatComparison,
//     Operator::F64Le => InstructionType::FloatComparison,
//     Operator::F64Ge => InstructionType::FloatComparison,

//     Operator::I32Clz => InstructionType::Bit,
//     Operator::I32Ctz => InstructionType::Bit,
//     Operator::I32Popcnt => InstructionType::Bit,
//     Operator::I32Add => InstructionType::Add,
//     Operator::I32Sub => InstructionType::Add,
//     Operator::I32Mul => InstructionType::Mul,
//     Operator::I32DivS => InstructionType::Div,
//     Operator::I32DivU => InstructionType::Div,
//     Operator::I32RemS => InstructionType::Div,
//     Operator::I32RemU => InstructionType::Div,
//     Operator::I32And => InstructionType::Bit,
//     Operator::I32Or => InstructionType::Bit,
//     Operator::I32Xor => InstructionType::Bit,
//     Operator::I32Shl => InstructionType::Bit,
//     Operator::I32ShrS => InstructionType::Bit,
//     Operator::I32ShrU => InstructionType::Bit,
//     Operator::I32Rotl => InstructionType::Bit,
//     Operator::I32Rotr => InstructionType::Bit,

//     Operator::I64Clz => InstructionType::Bit,
//     Operator::I64Ctz => InstructionType::Bit,
//     Operator::I64Popcnt => InstructionType::Bit,
//     Operator::I64Add => InstructionType::Add,
//     Operator::I64Sub => InstructionType::Add,
//     Operator::I64Mul => InstructionType::Mul,
//     Operator::I64DivS => InstructionType::Div,
//     Operator::I64DivU => InstructionType::Div,
//     Operator::I64RemS => InstructionType::Div,
//     Operator::I64RemU => InstructionType::Div,
//     Operator::I64And => InstructionType::Bit,
//     Operator::I64Or => InstructionType::Bit,
//     Operator::I64Xor => InstructionType::Bit,
//     Operator::I64Shl => InstructionType::Bit,
//     Operator::I64ShrS => InstructionType::Bit,
//     Operator::I64ShrU => InstructionType::Bit,
//     Operator::I64Rotl => InstructionType::Bit,
//     Operator::I64Rotr => InstructionType::Bit,

//     Operator::F32Abs => InstructionType::Float,
//     Operator::F32Neg => InstructionType::Float,
//     Operator::F32Ceil => InstructionType::Float,
//     Operator::F32Floor => InstructionType::Float,
//     Operator::F32Trunc => InstructionType::Float,
//     Operator::F32Nearest => InstructionType::Float,
//     Operator::F32Sqrt => InstructionType::Float,
//     Operator::F32Add => InstructionType::Float,
//     Operator::F32Sub => InstructionType::Float,
//     Operator::F32Mul => InstructionType::Float,
//     Operator::F32Div => InstructionType::Float,
//     Operator::F32Min => InstructionType::Float,
//     Operator::F32Max => InstructionType::Float,
//     Operator::F32Copysign => InstructionType::Float,
//     Operator::F64Abs => InstructionType::Float,
//     Operator::F64Neg => InstructionType::Float,
//     Operator::F64Ceil => InstructionType::Float,
//     Operator::F64Floor => InstructionType::Float,
//     Operator::F64Trunc => InstructionType::Float,
//     Operator::F64Nearest => InstructionType::Float,
//     Operator::F64Sqrt => InstructionType::Float,
//     Operator::F64Add => InstructionType::Float,
//     Operator::F64Sub => InstructionType::Float,
//     Operator::F64Mul => InstructionType::Float,
//     Operator::F64Div => InstructionType::Float,
//     Operator::F64Min => InstructionType::Float,
//     Operator::F64Max => InstructionType::Float,
//     Operator::F64Copysign => InstructionType::Float,

//     Operator::I32WrapI64 => InstructionType::Conversion,
//     Operator::I64ExtendI32S => InstructionType::Conversion,
//     Operator::I64ExtendI32U => InstructionType::Conversion,

//     Operator::I32TruncF32S => InstructionType::FloatConversion,
//     Operator::I32TruncF32U => InstructionType::FloatConversion,
//     Operator::I32TruncF64S => InstructionType::FloatConversion,
//     Operator::I32TruncF64U => InstructionType::FloatConversion,
//     Operator::I64TruncF32S => InstructionType::FloatConversion,
//     Operator::I64TruncF32U => InstructionType::FloatConversion,
//     Operator::I64TruncF64S => InstructionType::FloatConversion,
//     Operator::I64TruncF64U => InstructionType::FloatConversion,
//     Operator::F32ConvertI32S => InstructionType::FloatConversion,
//     Operator::F32ConvertI32U => InstructionType::FloatConversion,
//     Operator::F32ConvertI64S => InstructionType::FloatConversion,
//     Operator::F32ConvertI64U => InstructionType::FloatConversion,
//     Operator::F32DemoteF64 => InstructionType::FloatConversion,
//     Operator::F64ConvertI32S => InstructionType::FloatConversion,
//     Operator::F64ConvertI32U => InstructionType::FloatConversion,
//     Operator::F64ConvertI64S => InstructionType::FloatConversion,
//     Operator::F64ConvertI64U => InstructionType::FloatConversion,
//     Operator::F64PromoteF32 => InstructionType::FloatConversion,

//     Operator::I32ReinterpretF32 => InstructionType::Reinterpretation,
//     Operator::I64ReinterpretF64 => InstructionType::Reinterpretation,
//     Operator::F32ReinterpretI32 => InstructionType::Reinterpretation,
//     Operator::F64ReinterpretI64 => InstructionType::Reinterpretation,

//     // NOTEL: Those are unsupported proposals. These opcodes should be disabled by another
//     // wasmer middleware.
//     Operator::Try { .. } => todo!(),
//     Operator::Catch { .. } => todo!(),
//     Operator::Throw { .. } => todo!(),
//     Operator::Rethrow { relative_depth: _ } => todo!(),
//     Operator::ReturnCall { .. } => todo!(),
//     Operator::ReturnCallIndirect { .. } => todo!(),
//     Operator::Delegate { relative_depth: _ } => todo!(),
//     Operator::CatchAll => todo!(),
//     Operator::TypedSelect { ty: _ } => todo!(),
//     Operator::RefNull { ty: _ } => todo!(),
//     Operator::RefIsNull => todo!(),
//     Operator::RefFunc { function_index: _ } => todo!(),

//     Operator::I32Extend8S => todo!(),
//     Operator::I32Extend16S => todo!(),
//     Operator::I64Extend8S => todo!(),
//     Operator::I64Extend16S => todo!(),
//     Operator::I64Extend32S => todo!(),
//     Operator::I32TruncSatF32S => todo!(),
//     Operator::I32TruncSatF32U => todo!(),
//     Operator::I32TruncSatF64S => todo!(),
//     Operator::I32TruncSatF64U => todo!(),
//     Operator::I64TruncSatF32S => todo!(),
//     Operator::I64TruncSatF32U => todo!(),
//     Operator::I64TruncSatF64S => todo!(),
//     Operator::I64TruncSatF64U => todo!(),
//     Operator::MemoryInit { .. } => todo!(),
//     Operator::DataDrop { .. } => todo!(),
//     Operator::MemoryCopy { .. } => todo!(),
//     Operator::MemoryFill { mem: _ } => todo!(),
//     Operator::TableInit { .. } => todo!(),
//     Operator::ElemDrop { .. } => todo!(),
//     Operator::TableCopy {
//         dst_table: _,
//         src_table: _,
//     } => todo!(),
//     Operator::TableFill { table: _ } => todo!(),
//     Operator::TableGet { table: _ } => todo!(),
//     Operator::TableSet { table: _ } => todo!(),
//     Operator::TableGrow { table: _ } => todo!(),
//     Operator::TableSize { table: _ } => todo!(),
//     Operator::MemoryAtomicNotify { memarg: _ } => todo!(),
//     Operator::MemoryAtomicWait32 { memarg: _ } => todo!(),
//     Operator::MemoryAtomicWait64 { memarg: _ } => todo!(),
//     Operator::AtomicFence { .. } => todo!(),
//     Operator::I32AtomicLoad { memarg: _ } => todo!(),
//     Operator::I64AtomicLoad { memarg: _ } => todo!(),
//     Operator::I32AtomicLoad8U { memarg: _ } => todo!(),
//     Operator::I32AtomicLoad16U { memarg: _ } => todo!(),
//     Operator::I64AtomicLoad8U { memarg: _ } => todo!(),
//     Operator::I64AtomicLoad16U { memarg: _ } => todo!(),
//     Operator::I64AtomicLoad32U { memarg: _ } => todo!(),
//     Operator::I32AtomicStore { memarg: _ } => todo!(),
//     Operator::I64AtomicStore { memarg: _ } => todo!(),
//     Operator::I32AtomicStore8 { memarg: _ } => todo!(),
//     Operator::I32AtomicStore16 { memarg: _ } => todo!(),
//     Operator::I64AtomicStore8 { memarg: _ } => todo!(),
//     Operator::I64AtomicStore16 { memarg: _ } => todo!(),
//     Operator::I64AtomicStore32 { memarg: _ } => todo!(),
//     Operator::I32AtomicRmwAdd { memarg: _ } => todo!(),
//     Operator::I64AtomicRmwAdd { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw8AddU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw16AddU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw8AddU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw16AddU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw32AddU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmwSub { memarg: _ } => todo!(),
//     Operator::I64AtomicRmwSub { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw8SubU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw16SubU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw8SubU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw16SubU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw32SubU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmwAnd { memarg: _ } => todo!(),
//     Operator::I64AtomicRmwAnd { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw8AndU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw16AndU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw8AndU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw16AndU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw32AndU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmwOr { memarg: _ } => todo!(),
//     Operator::I64AtomicRmwOr { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw8OrU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw16OrU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw8OrU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw16OrU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw32OrU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmwXor { memarg: _ } => todo!(),
//     Operator::I64AtomicRmwXor { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw8XorU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw16XorU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw8XorU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw16XorU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw32XorU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmwXchg { memarg: _ } => todo!(),
//     Operator::I64AtomicRmwXchg { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw8XchgU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw16XchgU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw8XchgU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw16XchgU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw32XchgU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmwCmpxchg { memarg: _ } => todo!(),
//     Operator::I64AtomicRmwCmpxchg { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw8CmpxchgU { memarg: _ } => todo!(),
//     Operator::I32AtomicRmw16CmpxchgU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw8CmpxchgU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw16CmpxchgU { memarg: _ } => todo!(),
//     Operator::I64AtomicRmw32CmpxchgU { memarg: _ } => todo!(),
//     Operator::V128Load { memarg: _ } => todo!(),
//     Operator::V128Load8x8S { memarg: _ } => todo!(),
//     Operator::V128Load8x8U { memarg: _ } => todo!(),
//     Operator::V128Load16x4S { memarg: _ } => todo!(),
//     Operator::V128Load16x4U { memarg: _ } => todo!(),
//     Operator::V128Load32x2S { memarg: _ } => todo!(),
//     Operator::V128Load32x2U { memarg: _ } => todo!(),
//     Operator::V128Load8Splat { memarg: _ } => todo!(),
//     Operator::V128Load16Splat { memarg: _ } => todo!(),
//     Operator::V128Load32Splat { memarg: _ } => todo!(),
//     Operator::V128Load64Splat { memarg: _ } => todo!(),
//     Operator::V128Load32Zero { memarg: _ } => todo!(),
//     Operator::V128Load64Zero { memarg: _ } => todo!(),
//     Operator::V128Store { memarg: _ } => todo!(),
//     Operator::V128Load8Lane { memarg: _, lane: _ } => todo!(),
//     Operator::V128Load16Lane { memarg: _, lane: _ } => todo!(),
//     Operator::V128Load32Lane { memarg: _, lane: _ } => todo!(),
//     Operator::V128Load64Lane { memarg: _, lane: _ } => todo!(),
//     Operator::V128Store8Lane { memarg: _, lane: _ } => todo!(),
//     Operator::V128Store16Lane { memarg: _, lane: _ } => todo!(),
//     Operator::V128Store32Lane { memarg: _, lane: _ } => todo!(),
//     Operator::V128Store64Lane { memarg: _, lane: _ } => todo!(),
//     Operator::V128Const { value: _ } => todo!(),
//     Operator::I8x16Shuffle { lanes: _ } => todo!(),
//     Operator::I8x16ExtractLaneS { lane: _ } => todo!(),
//     Operator::I8x16ExtractLaneU { lane: _ } => todo!(),
//     Operator::I8x16ReplaceLane { lane: _ } => todo!(),
//     Operator::I16x8ExtractLaneS { lane: _ } => todo!(),
//     Operator::I16x8ExtractLaneU { lane: _ } => todo!(),
//     Operator::I16x8ReplaceLane { lane: _ } => todo!(),
//     Operator::I32x4ExtractLane { lane: _ } => todo!(),
//     Operator::I32x4ReplaceLane { lane: _ } => todo!(),
//     Operator::I64x2ExtractLane { lane: _ } => todo!(),
//     Operator::I64x2ReplaceLane { lane: _ } => todo!(),
//     Operator::F32x4ExtractLane { lane: _ } => todo!(),
//     Operator::F32x4ReplaceLane { lane: _ } => todo!(),
//     Operator::F64x2ExtractLane { lane: _ } => todo!(),
//     Operator::F64x2ReplaceLane { lane: _ } => todo!(),
//     Operator::I8x16Swizzle => todo!(),
//     Operator::I8x16Splat => todo!(),
//     Operator::I16x8Splat => todo!(),
//     Operator::I32x4Splat => todo!(),
//     Operator::I64x2Splat => todo!(),
//     Operator::F32x4Splat => todo!(),
//     Operator::F64x2Splat => todo!(),
//     Operator::I8x16Eq => todo!(),
//     Operator::I8x16Ne => todo!(),
//     Operator::I8x16LtS => todo!(),
//     Operator::I8x16LtU => todo!(),
//     Operator::I8x16GtS => todo!(),
//     Operator::I8x16GtU => todo!(),
//     Operator::I8x16LeS => todo!(),
//     Operator::I8x16LeU => todo!(),
//     Operator::I8x16GeS => todo!(),
//     Operator::I8x16GeU => todo!(),
//     Operator::I16x8Eq => todo!(),
//     Operator::I16x8Ne => todo!(),
//     Operator::I16x8LtS => todo!(),
//     Operator::I16x8LtU => todo!(),
//     Operator::I16x8GtS => todo!(),
//     Operator::I16x8GtU => todo!(),
//     Operator::I16x8LeS => todo!(),
//     Operator::I16x8LeU => todo!(),
//     Operator::I16x8GeS => todo!(),
//     Operator::I16x8GeU => todo!(),
//     Operator::I32x4Eq => todo!(),
//     Operator::I32x4Ne => todo!(),
//     Operator::I32x4LtS => todo!(),
//     Operator::I32x4LtU => todo!(),
//     Operator::I32x4GtS => todo!(),
//     Operator::I32x4GtU => todo!(),
//     Operator::I32x4LeS => todo!(),
//     Operator::I32x4LeU => todo!(),
//     Operator::I32x4GeS => todo!(),
//     Operator::I32x4GeU => todo!(),
//     Operator::I64x2Eq => todo!(),
//     Operator::I64x2Ne => todo!(),
//     Operator::I64x2LtS => todo!(),
//     Operator::I64x2GtS => todo!(),
//     Operator::I64x2LeS => todo!(),
//     Operator::I64x2GeS => todo!(),
//     Operator::F32x4Eq => todo!(),
//     Operator::F32x4Ne => todo!(),
//     Operator::F32x4Lt => todo!(),
//     Operator::F32x4Gt => todo!(),
//     Operator::F32x4Le => todo!(),
//     Operator::F32x4Ge => todo!(),
//     Operator::F64x2Eq => todo!(),
//     Operator::F64x2Ne => todo!(),
//     Operator::F64x2Lt => todo!(),
//     Operator::F64x2Gt => todo!(),
//     Operator::F64x2Le => todo!(),
//     Operator::F64x2Ge => todo!(),
//     Operator::V128Not => todo!(),
//     Operator::V128And => todo!(),
//     Operator::V128AndNot => todo!(),
//     Operator::V128Or => todo!(),
//     Operator::V128Xor => todo!(),
//     Operator::V128Bitselect => todo!(),
//     Operator::V128AnyTrue => todo!(),
//     Operator::I8x16Abs => todo!(),
//     Operator::I8x16Neg => todo!(),
//     Operator::I8x16Popcnt => todo!(),
//     Operator::I8x16AllTrue => todo!(),
//     Operator::I8x16Bitmask => todo!(),
//     Operator::I8x16NarrowI16x8S => todo!(),
//     Operator::I8x16NarrowI16x8U => todo!(),
//     Operator::I8x16Shl => todo!(),
//     Operator::I8x16ShrS => todo!(),
//     Operator::I8x16ShrU => todo!(),
//     Operator::I8x16Add => todo!(),
//     Operator::I8x16AddSatS => todo!(),
//     Operator::I8x16AddSatU => todo!(),
//     Operator::I8x16Sub => todo!(),
//     Operator::I8x16SubSatS => todo!(),
//     Operator::I8x16SubSatU => todo!(),
//     Operator::I8x16MinS => todo!(),
//     Operator::I8x16MinU => todo!(),
//     Operator::I8x16MaxS => todo!(),
//     Operator::I8x16MaxU => todo!(),
//     // Operator::I8x16RoundingAverageU => todo!(),
//     Operator::I16x8ExtAddPairwiseI8x16S => todo!(),
//     Operator::I16x8ExtAddPairwiseI8x16U => todo!(),
//     Operator::I16x8Abs => todo!(),
//     Operator::I16x8Neg => todo!(),
//     Operator::I16x8Q15MulrSatS => todo!(),
//     Operator::I16x8AllTrue => todo!(),
//     Operator::I16x8Bitmask => todo!(),
//     Operator::I16x8NarrowI32x4S => todo!(),
//     Operator::I16x8NarrowI32x4U => todo!(),
//     Operator::I16x8ExtendLowI8x16S => todo!(),
//     Operator::I16x8ExtendHighI8x16S => todo!(),
//     Operator::I16x8ExtendLowI8x16U => todo!(),
//     Operator::I16x8ExtendHighI8x16U => todo!(),
//     Operator::I16x8Shl => todo!(),
//     Operator::I16x8ShrS => todo!(),
//     Operator::I16x8ShrU => todo!(),
//     Operator::I16x8Add => todo!(),
//     Operator::I16x8AddSatS => todo!(),
//     Operator::I16x8AddSatU => todo!(),
//     Operator::I16x8Sub => todo!(),
//     Operator::I16x8SubSatS => todo!(),
//     Operator::I16x8SubSatU => todo!(),
//     Operator::I16x8Mul => todo!(),
//     Operator::I16x8MinS => todo!(),
//     Operator::I16x8MinU => todo!(),
//     Operator::I16x8MaxS => todo!(),
//     Operator::I16x8MaxU => todo!(),
//     // Operator::I16x8RoundingAverageU => todo!(),
//     Operator::I16x8ExtMulLowI8x16S => todo!(),
//     Operator::I16x8ExtMulHighI8x16S => todo!(),
//     Operator::I16x8ExtMulLowI8x16U => todo!(),
//     Operator::I16x8ExtMulHighI8x16U => todo!(),
//     Operator::I32x4ExtAddPairwiseI16x8S => todo!(),
//     Operator::I32x4ExtAddPairwiseI16x8U => todo!(),
//     Operator::I32x4Abs => todo!(),
//     Operator::I32x4Neg => todo!(),
//     Operator::I32x4AllTrue => todo!(),
//     Operator::I32x4Bitmask => todo!(),
//     Operator::I32x4ExtendLowI16x8S => todo!(),
//     Operator::I32x4ExtendHighI16x8S => todo!(),
//     Operator::I32x4ExtendLowI16x8U => todo!(),
//     Operator::I32x4ExtendHighI16x8U => todo!(),
//     Operator::I32x4Shl => todo!(),
//     Operator::I32x4ShrS => todo!(),
//     Operator::I32x4ShrU => todo!(),
//     Operator::I32x4Add => todo!(),
//     Operator::I32x4Sub => todo!(),
//     Operator::I32x4Mul => todo!(),
//     Operator::I32x4MinS => todo!(),
//     Operator::I32x4MinU => todo!(),
//     Operator::I32x4MaxS => todo!(),
//     Operator::I32x4MaxU => todo!(),
//     Operator::I32x4DotI16x8S => todo!(),
//     Operator::I32x4ExtMulLowI16x8S => todo!(),
//     Operator::I32x4ExtMulHighI16x8S => todo!(),
//     Operator::I32x4ExtMulLowI16x8U => todo!(),
//     Operator::I32x4ExtMulHighI16x8U => todo!(),
//     Operator::I64x2Abs => todo!(),
//     Operator::I64x2Neg => todo!(),
//     Operator::I64x2AllTrue => todo!(),
//     Operator::I64x2Bitmask => todo!(),
//     Operator::I64x2ExtendLowI32x4S => todo!(),
//     Operator::I64x2ExtendHighI32x4S => todo!(),
//     Operator::I64x2ExtendLowI32x4U => todo!(),
//     Operator::I64x2ExtendHighI32x4U => todo!(),
//     Operator::I64x2Shl => todo!(),
//     Operator::I64x2ShrS => todo!(),
//     Operator::I64x2ShrU => todo!(),
//     Operator::I64x2Add => todo!(),
//     Operator::I64x2Sub => todo!(),
//     Operator::I64x2Mul => todo!(),
//     Operator::I64x2ExtMulLowI32x4S => todo!(),
//     Operator::I64x2ExtMulHighI32x4S => todo!(),
//     Operator::I64x2ExtMulLowI32x4U => todo!(),
//     Operator::I64x2ExtMulHighI32x4U => todo!(),
//     Operator::F32x4Ceil => todo!(),
//     Operator::F32x4Floor => todo!(),
//     Operator::F32x4Trunc => todo!(),
//     Operator::F32x4Nearest => todo!(),
//     Operator::F32x4Abs => todo!(),
//     Operator::F32x4Neg => todo!(),
//     Operator::F32x4Sqrt => todo!(),
//     Operator::F32x4Add => todo!(),
//     Operator::F32x4Sub => todo!(),
//     Operator::F32x4Mul => todo!(),
//     Operator::F32x4Div => todo!(),
//     Operator::F32x4Min => todo!(),
//     Operator::F32x4Max => todo!(),
//     Operator::F32x4PMin => todo!(),
//     Operator::F32x4PMax => todo!(),
//     Operator::F64x2Ceil => todo!(),
//     Operator::F64x2Floor => todo!(),
//     Operator::F64x2Trunc => todo!(),
//     Operator::F64x2Nearest => todo!(),
//     Operator::F64x2Abs => todo!(),
//     Operator::F64x2Neg => todo!(),
//     Operator::F64x2Sqrt => todo!(),
//     Operator::F64x2Add => todo!(),
//     Operator::F64x2Sub => todo!(),
//     Operator::F64x2Mul => todo!(),
//     Operator::F64x2Div => todo!(),
//     Operator::F64x2Min => todo!(),
//     Operator::F64x2Max => todo!(),
//     Operator::F64x2PMin => todo!(),
//     Operator::F64x2PMax => todo!(),
//     Operator::I32x4TruncSatF32x4S => todo!(),
//     Operator::I32x4TruncSatF32x4U => todo!(),
//     Operator::F32x4ConvertI32x4S => todo!(),
//     Operator::F32x4ConvertI32x4U => todo!(),
//     Operator::I32x4TruncSatF64x2SZero => todo!(),
//     Operator::I32x4TruncSatF64x2UZero => todo!(),
//     Operator::F64x2ConvertLowI32x4S => todo!(),
//     Operator::F64x2ConvertLowI32x4U => todo!(),
//     Operator::F32x4DemoteF64x2Zero => todo!(),
//     Operator::F64x2PromoteLowF32x4 => todo!(),
//     Operator::I8x16RelaxedSwizzle => todo!(),
//     Operator::I32x4RelaxedTruncSatF32x4S => todo!(),
//     Operator::I32x4RelaxedTruncSatF32x4U => todo!(),
//     Operator::I32x4RelaxedTruncSatF64x2SZero => todo!(),
//     Operator::I32x4RelaxedTruncSatF64x2UZero => todo!(),
//     // Operator::F32x4Fma => todo!(),
//     // Operator::F32x4Fms => todo!(),
//     // Operator::F64x2Fma => todo!(),
//     // Operator::F64x2Fms => todo!(),
//     // Operator::I8x16LaneSelect => todo!(),
//     // Operator::I16x8LaneSelect => todo!(),
//     // Operator::I32x4LaneSelect => todo!(),
//     // Operator::I64x2LaneSelect => todo!(),
//     Operator::F32x4RelaxedMin => todo!(),
//     Operator::F32x4RelaxedMax => todo!(),
//     Operator::F64x2RelaxedMin => todo!(),
//     Operator::F64x2RelaxedMax => todo!(),
// };
// // dbg!(&instruction_type);

// let cost = match instruction_type {
//     InstructionType::Bit => opcode_costs.bit,
//     InstructionType::Add => opcode_costs.add,
//     InstructionType::Mul => opcode_costs.mul,
//     InstructionType::Div => opcode_costs.div,
//     InstructionType::Load => opcode_costs.load,
//     InstructionType::Store => opcode_costs.store,
//     InstructionType::Const => opcode_costs.op_const,
//     InstructionType::FloatConst => opcode_costs.regular, //todo!("opcode_costs.float_const"),
//     InstructionType::Local => opcode_costs.local,
//     InstructionType::Global => opcode_costs.global,
//     InstructionType::ControlFlow => opcode_costs.control_flow,
//     InstructionType::IntegerComparison => opcode_costs.integer_comparison,
//     InstructionType::FloatComparison => opcode_costs.regular, /* todo!("opcode_costs. */
//     // float_comparison"),
//     InstructionType::Float => opcode_costs.regular, //todo!("opcode_costs.float"),
//     InstructionType::Conversion => opcode_costs.conversion,
//     InstructionType::FloatConversion => opcode_costs.regular, /* todo!("opcode_costs. */
//     // float_conversion"),
//     InstructionType::Reinterpretation => {
//         // missing entry for reinterpretation, falling back to regular
//         opcode_costs.regular
//     }
//     InstructionType::Unreachable => opcode_costs.unreachable,
//     InstructionType::Nop => opcode_costs.nop,
//     InstructionType::CurrentMemory => opcode_costs.current_memory,
//     InstructionType::GrowMemory(_mem) => opcode_costs.grow_memory,
// };
// dbg!(&cost);
// cost.into()
// 1 // useful for debugging how many instructions were executed
// }

pub(crate) fn make_wasmer_metering_middleware(initial_limit: u64) -> Arc<dyn ModuleMiddleware> {
    Arc::new(Metering::new(initial_limit, move |operator| {
        // cost_function(opcode_costs, operator)
        1 // for debugging
    }))
}

// #[cfg(test)]
// mod tests {
//     use crate::shared::opcode_costs::OpcodeCosts;

//     #[test]
//     fn should_create_metering_middleware() {
//         let _middleware = super::make_wasmer_metering_middleware(u64::MAX,
// OpcodeCosts::default());     }
// }
