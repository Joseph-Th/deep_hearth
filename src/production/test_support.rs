//! Unit-test conveniences layered over the public production resolution surface.

use crate::core::time::TickSpan;
use crate::material::MaterialLotSpec;

use super::{
    ProcessOutputStream, ProcessOutputStreamId, ProcessResolution, ValidatedProcessInputs,
};

pub(crate) fn make_test_process_resolution_with_streams(
    inputs: ValidatedProcessInputs,
    duration_ticks: u64,
    output_streams: Vec<(ProcessOutputStreamId, Vec<MaterialLotSpec>)>,
) -> ProcessResolution {
    let output_streams = output_streams
        .into_iter()
        .map(|(id, outputs)| ProcessOutputStream::new(id, outputs))
        .collect();
    match inputs.resolve_without_resources_routed(TickSpan::new(duration_ticks), output_streams) {
        Ok(resolution) => resolution,
        Err(error) => panic!("multi-stream test process resolution fixture failed: {error}"),
    }
}

pub(crate) fn make_test_process_resolution(
    inputs: ValidatedProcessInputs,
    duration_ticks: u64,
    outputs: Vec<MaterialLotSpec>,
) -> ProcessResolution {
    match inputs.resolve_without_resources(TickSpan::new(duration_ticks), outputs) {
        Ok(resolution) => resolution,
        Err(error) => panic!("test process resolution fixture failed: {error}"),
    }
}

impl ProcessResolution {
    pub(crate) fn outputs(&self) -> &[MaterialLotSpec] {
        match self.single_output_stream() {
            Some(stream) => stream.outputs(),
            None => panic!("single-stream test support used with multi-stream process resolution"),
        }
    }
}
