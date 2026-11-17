//! DWG files, read by the `acadrust` crate (MPL-2.0, pure Rust).
//!
//! The decoded document is written to binary DXF in memory and read by our
//! DXF reader, so DWG and DXF drawings take the same path (blocks, object
//! coordinate systems, colors, limits). `acadrust` is not written to our
//! robustness rules (it may panic on damaged files), so it runs on its own
//! thread with a large stack, inside `catch_unwind`; a panic becomes an
//! error and the program goes on. Its memory-mapped reading (`unsafe`) is
//! not used: the file is read from a byte buffer.

use crate::{CadError, CadRead, Format, Limits};

/// Stack of the reading thread (deeply nested DWG objects recurse).
const STACK_BYTES: usize = 128 << 20;
/// Decompressed bytes one DWG read may allocate.
const MAX_DECOMPRESSED: usize = 1 << 30;

/// Decode a DWG and write it as binary DXF.
fn to_dxf(data: Vec<u8>) -> Result<(Vec<u8>, String), String> {
    use acadrust::{DwgReadOptions, DwgReader, DxfWriter};
    let options =
        DwgReadOptions::failsafe().with_max_decompressed_bytes(MAX_DECOMPRESSED).with_mesh_limits(100_000, 400_000);
    let mut reader = DwgReader::from_stream_with_options(std::io::Cursor::new(data), options);
    let mut doc = reader.read().map_err(|e| e.to_string())?;
    let version = format!("{:?}", doc.version);
    match DxfWriter::new_binary(&doc).write_to_vec() {
        Ok(bytes) => Ok((bytes, version)),
        Err(_) => {
            // Objects the library could not decode (e.g. constraint groups)
            // cannot be written as DXF; the drawing does not need them.
            doc.objects.retain(|_, o| !matches!(o, acadrust::objects::ObjectType::Unknown { .. }));
            DxfWriter::new_binary(&doc).write_to_vec().map(|b| (b, version)).map_err(|e| e.to_string())
        }
    }
}

pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "internal error".to_string())
}

pub fn read(data: &[u8], limits: &Limits) -> Result<CadRead, CadError> {
    let owned = data.to_vec();
    let worker = std::thread::Builder::new()
        .name("dwg-reader".into())
        .stack_size(STACK_BYTES)
        .spawn(move || std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| to_dxf(owned))))
        .map_err(|e| CadError::Dwg(e.to_string()))?;
    let (dxf, version) = match worker.join() {
        Ok(Ok(Ok(out))) => out,
        Ok(Ok(Err(message))) => return Err(CadError::Dwg(message)),
        Ok(Err(payload)) | Err(payload) => return Err(CadError::Dwg(panic_message(payload.as_ref()))),
    };
    // The DXF of a large DWG may exceed the DXF file limit; it is ours, so read it whole.
    let inner = Limits { max_dxf_bytes: usize::MAX, ..limits.clone() };
    let mut read = crate::dxf::read(&dxf, &inner)?;
    read.format = Format::Dwg;
    read.version = version;
    Ok(read)
}
