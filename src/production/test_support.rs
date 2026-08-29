//! Unit-test conveniences layered over the public production resolution surface.

use crate::material::MaterialLotSpec;

use super::ProcessResolution;

impl ProcessResolution {
    pub(crate) fn outputs(&self) -> &[MaterialLotSpec] {
        match self.single_output_stream() {
            Some(stream) => stream.outputs(),
            None => panic!("single-stream test support used with multi-stream process resolution"),
        }
    }
}
