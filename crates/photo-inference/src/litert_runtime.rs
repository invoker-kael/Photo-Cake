use libloading::Library;
use std::ffi::{CString, c_char, c_void};
use std::mem::size_of;
use std::path::{Path, PathBuf};
use thiserror::Error;

const STATUS_OK: i32 = 0;
const ELEMENT_TYPE_FLOAT32: i32 = 1;
const TENSOR_BUFFER_HOST_MEMORY: i32 = 1;
const TENSOR_BUFFER_LOCK_READ: i32 = 0;
const TENSOR_BUFFER_LOCK_WRITE: i32 = 1;
const HW_ACCELERATOR_CPU: i32 = 1;
const MAX_RANK: usize = 8;

type Handle = *mut c_void;
type Status = i32;
type ParamIndex = usize;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LiteRtLayout {
    flags: u32,
    dimensions: [i32; MAX_RANK],
    strides: [u32; MAX_RANK],
}

impl Default for LiteRtLayout {
    fn default() -> Self {
        Self {
            flags: 0,
            dimensions: [0; MAX_RANK],
            strides: [0; MAX_RANK],
        }
    }
}

impl LiteRtLayout {
    fn rank(&self) -> usize {
        (self.flags & 0x7f) as usize
    }

    fn dimensions(&self) -> Result<Vec<i32>, LiteRtRuntimeError> {
        let rank = self.rank();
        if rank == 0 || rank > MAX_RANK {
            return Err(LiteRtRuntimeError::InvalidTensor(format!(
                "invalid tensor rank {rank}"
            )));
        }
        let dimensions = self.dimensions[..rank].to_vec();
        if dimensions.iter().any(|dimension| *dimension <= 0) {
            return Err(LiteRtRuntimeError::InvalidTensor(format!(
                "dynamic/invalid dimensions {dimensions:?} are not supported in the fixed Photo-Cake models"
            )));
        }
        Ok(dimensions)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct LiteRtRankedTensorType {
    element_type: i32,
    layout: LiteRtLayout,
}

type CreateEnvironmentFn = unsafe extern "C" fn(i32, *const c_void, *mut Handle) -> Status;
type DestroyEnvironmentFn = unsafe extern "C" fn(Handle);
type CreateModelFromFileFn = unsafe extern "C" fn(Handle, *const c_char, *mut Handle) -> Status;
type DestroyModelFn = unsafe extern "C" fn(Handle);
type GetMainModelSubgraphIndexFn = unsafe extern "C" fn(Handle, *mut ParamIndex) -> Status;
type GetModelSubgraphFn = unsafe extern "C" fn(Handle, ParamIndex, *mut Handle) -> Status;
type GetNumSubgraphInputsFn = unsafe extern "C" fn(Handle, *mut ParamIndex) -> Status;
type GetSubgraphInputFn = unsafe extern "C" fn(Handle, ParamIndex, *mut Handle) -> Status;
type GetNumSubgraphOutputsFn = unsafe extern "C" fn(Handle, *mut ParamIndex) -> Status;
type GetSubgraphOutputFn = unsafe extern "C" fn(Handle, ParamIndex, *mut Handle) -> Status;
type GetRankedTensorTypeFn =
    unsafe extern "C" fn(Handle, *mut LiteRtRankedTensorType) -> Status;
type CreateOptionsFn = unsafe extern "C" fn(*mut Handle) -> Status;
type DestroyOptionsFn = unsafe extern "C" fn(Handle);
type SetOptionsHardwareAcceleratorsFn = unsafe extern "C" fn(Handle, i32) -> Status;
type CreateCompiledModelFn = unsafe extern "C" fn(Handle, Handle, Handle, *mut Handle) -> Status;
type DestroyCompiledModelFn = unsafe extern "C" fn(Handle);
type CreateManagedTensorBufferFn = unsafe extern "C" fn(
    Handle,
    i32,
    *const LiteRtRankedTensorType,
    usize,
    *mut Handle,
) -> Status;
type DestroyTensorBufferFn = unsafe extern "C" fn(Handle);
type LockTensorBufferFn = unsafe extern "C" fn(Handle, *mut *mut c_void, i32) -> Status;
type UnlockTensorBufferFn = unsafe extern "C" fn(Handle) -> Status;
type RunCompiledModelFn = unsafe extern "C" fn(
    Handle,
    ParamIndex,
    usize,
    *mut Handle,
    usize,
    *mut Handle,
) -> Status;

#[derive(Debug, Error)]
pub(crate) enum LiteRtRuntimeError {
    #[error("LiteRT runtime is missing: {0}")]
    MissingRuntime(PathBuf),
    #[error("failed to load LiteRT runtime {path}: {message}")]
    LoadRuntime { path: PathBuf, message: String },
    #[error("LiteRT runtime is missing symbol {symbol}: {message}")]
    MissingSymbol { symbol: String, message: String },
    #[error("LiteRT {operation} failed with status {status}")]
    Status { operation: &'static str, status: Status },
    #[error("invalid LiteRT tensor: {0}")]
    InvalidTensor(String),
    #[error("model path is not valid UTF-8: {0}")]
    InvalidModelPath(PathBuf),
    #[error("model path contains an embedded NUL byte: {0}")]
    InvalidModelCString(PathBuf),
}

struct LiteRtApi {
    _library: Library,
    create_environment: CreateEnvironmentFn,
    destroy_environment: DestroyEnvironmentFn,
    create_model_from_file: CreateModelFromFileFn,
    destroy_model: DestroyModelFn,
    get_main_model_subgraph_index: GetMainModelSubgraphIndexFn,
    get_model_subgraph: GetModelSubgraphFn,
    get_num_subgraph_inputs: GetNumSubgraphInputsFn,
    get_subgraph_input: GetSubgraphInputFn,
    get_num_subgraph_outputs: GetNumSubgraphOutputsFn,
    get_subgraph_output: GetSubgraphOutputFn,
    get_ranked_tensor_type: GetRankedTensorTypeFn,
    create_options: CreateOptionsFn,
    destroy_options: DestroyOptionsFn,
    set_options_hardware_accelerators: SetOptionsHardwareAcceleratorsFn,
    create_compiled_model: CreateCompiledModelFn,
    destroy_compiled_model: DestroyCompiledModelFn,
    create_managed_tensor_buffer: CreateManagedTensorBufferFn,
    destroy_tensor_buffer: DestroyTensorBufferFn,
    lock_tensor_buffer: LockTensorBufferFn,
    unlock_tensor_buffer: UnlockTensorBufferFn,
    run_compiled_model: RunCompiledModelFn,
}

impl LiteRtApi {
    fn load(path: &Path) -> Result<Self, LiteRtRuntimeError> {
        if !path.is_file() {
            return Err(LiteRtRuntimeError::MissingRuntime(path.to_path_buf()));
        }
        let library = unsafe { Library::new(path) }.map_err(|error| {
            LiteRtRuntimeError::LoadRuntime {
                path: path.to_path_buf(),
                message: error.to_string(),
            }
        })?;

        Ok(Self {
            create_environment: load_symbol(&library, b"LiteRtCreateEnvironment\0")?,
            destroy_environment: load_symbol(&library, b"LiteRtDestroyEnvironment\0")?,
            create_model_from_file: load_symbol(&library, b"LiteRtCreateModelFromFile\0")?,
            destroy_model: load_symbol(&library, b"LiteRtDestroyModel\0")?,
            get_main_model_subgraph_index: load_symbol(
                &library,
                b"LiteRtGetMainModelSubgraphIndex\0",
            )?,
            get_model_subgraph: load_symbol(&library, b"LiteRtGetModelSubgraph\0")?,
            get_num_subgraph_inputs: load_symbol(&library, b"LiteRtGetNumSubgraphInputs\0")?,
            get_subgraph_input: load_symbol(&library, b"LiteRtGetSubgraphInput\0")?,
            get_num_subgraph_outputs: load_symbol(&library, b"LiteRtGetNumSubgraphOutputs\0")?,
            get_subgraph_output: load_symbol(&library, b"LiteRtGetSubgraphOutput\0")?,
            get_ranked_tensor_type: load_symbol(&library, b"LiteRtGetRankedTensorType\0")?,
            create_options: load_symbol(&library, b"LiteRtCreateOptions\0")?,
            destroy_options: load_symbol(&library, b"LiteRtDestroyOptions\0")?,
            set_options_hardware_accelerators: load_symbol(
                &library,
                b"LiteRtSetOptionsHardwareAccelerators\0",
            )?,
            create_compiled_model: load_symbol(&library, b"LiteRtCreateCompiledModel\0")?,
            destroy_compiled_model: load_symbol(&library, b"LiteRtDestroyCompiledModel\0")?,
            create_managed_tensor_buffer: load_symbol(
                &library,
                b"LiteRtCreateManagedTensorBuffer\0",
            )?,
            destroy_tensor_buffer: load_symbol(&library, b"LiteRtDestroyTensorBuffer\0")?,
            lock_tensor_buffer: load_symbol(&library, b"LiteRtLockTensorBuffer\0")?,
            unlock_tensor_buffer: load_symbol(&library, b"LiteRtUnlockTensorBuffer\0")?,
            run_compiled_model: load_symbol(&library, b"LiteRtRunCompiledModel\0")?,
            _library: library,
        })
    }
}

fn load_symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T, LiteRtRuntimeError> {
    let symbol_name = String::from_utf8_lossy(name)
        .trim_end_matches('\0')
        .to_string();
    unsafe { library.get::<T>(name) }
        .map(|symbol| *symbol)
        .map_err(|error| LiteRtRuntimeError::MissingSymbol {
            symbol: symbol_name,
            message: error.to_string(),
        })
}

pub(crate) struct LiteRtSession {
    api: LiteRtApi,
    environment: Handle,
    model: Handle,
    options: Handle,
    compiled_model: Handle,
    input_types: Vec<LiteRtRankedTensorType>,
    output_types: Vec<LiteRtRankedTensorType>,
}

impl LiteRtSession {
    pub(crate) fn load(model_path: &Path) -> Result<Self, LiteRtRuntimeError> {
        let runtime_path = model_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(runtime_filename());
        let api = LiteRtApi::load(&runtime_path)?;

        let mut environment = std::ptr::null_mut();
        check(
            "create environment",
            unsafe { (api.create_environment)(0, std::ptr::null(), &mut environment) },
        )?;

        let model_path_text = model_path
            .to_str()
            .ok_or_else(|| LiteRtRuntimeError::InvalidModelPath(model_path.to_path_buf()))?;
        let model_path_c = CString::new(model_path_text)
            .map_err(|_| LiteRtRuntimeError::InvalidModelCString(model_path.to_path_buf()))?;
        let mut model = std::ptr::null_mut();
        if let Err(error) = check(
            "create model from file",
            unsafe { (api.create_model_from_file)(environment, model_path_c.as_ptr(), &mut model) },
        ) {
            unsafe { (api.destroy_environment)(environment) };
            return Err(error);
        }

        let mut options = std::ptr::null_mut();
        if let Err(error) = check("create options", unsafe { (api.create_options)(&mut options) }) {
            unsafe {
                (api.destroy_model)(model);
                (api.destroy_environment)(environment);
            }
            return Err(error);
        }
        if let Err(error) = check(
            "select CPU accelerator",
            unsafe { (api.set_options_hardware_accelerators)(options, HW_ACCELERATOR_CPU) },
        ) {
            unsafe {
                (api.destroy_options)(options);
                (api.destroy_model)(model);
                (api.destroy_environment)(environment);
            }
            return Err(error);
        }

        let mut compiled_model = std::ptr::null_mut();
        if let Err(error) = check(
            "create compiled model",
            unsafe { (api.create_compiled_model)(environment, model, options, &mut compiled_model) },
        ) {
            unsafe {
                (api.destroy_options)(options);
                (api.destroy_model)(model);
                (api.destroy_environment)(environment);
            }
            return Err(error);
        }

        let tensor_types = collect_tensor_types(&api, model);
        let (input_types, output_types) = match tensor_types {
            Ok(types) => types,
            Err(error) => {
                unsafe {
                    (api.destroy_compiled_model)(compiled_model);
                    (api.destroy_options)(options);
                    (api.destroy_model)(model);
                    (api.destroy_environment)(environment);
                }
                return Err(error);
            }
        };

        Ok(Self {
            api,
            environment,
            model,
            options,
            compiled_model,
            input_types,
            output_types,
        })
    }

    pub(crate) fn run_f32(
        &self,
        input: &[f32],
    ) -> Result<Vec<(Vec<i32>, Vec<f32>)>, LiteRtRuntimeError> {
        if self.input_types.len() != 1 {
            return Err(LiteRtRuntimeError::InvalidTensor(format!(
                "Photo-Cake fixed models expect one input, runtime reports {}",
                self.input_types.len()
            )));
        }

        let expected_input = tensor_elements(&self.input_types[0])?;
        if expected_input != input.len() {
            return Err(LiteRtRuntimeError::InvalidTensor(format!(
                "model input expects {expected_input} float elements with dimensions {:?}, got {}",
                dimensions(&self.input_types[0])?,
                input.len()
            )));
        }

        let mut input_buffers = Vec::with_capacity(self.input_types.len());
        let mut output_buffers = Vec::with_capacity(self.output_types.len());
        let result = (|| {
            for tensor_type in &self.input_types {
                input_buffers.push(self.create_buffer(tensor_type)?);
            }
            for tensor_type in &self.output_types {
                output_buffers.push(self.create_buffer(tensor_type)?);
            }

            self.write_buffer(input_buffers[0], input)?;
            check(
                "run compiled model",
                unsafe {
                    (self.api.run_compiled_model)(
                        self.compiled_model,
                        0,
                        input_buffers.len(),
                        input_buffers.as_mut_ptr(),
                        output_buffers.len(),
                        output_buffers.as_mut_ptr(),
                    )
                },
            )?;

            let mut outputs = Vec::with_capacity(output_buffers.len());
            for (buffer, tensor_type) in output_buffers.iter().zip(&self.output_types) {
                let length = tensor_elements(tensor_type)?;
                let values = self.read_buffer(*buffer, length)?;
                outputs.push((dimensions(tensor_type)?, values));
            }
            Ok(outputs)
        })();

        unsafe {
            for buffer in input_buffers.into_iter().chain(output_buffers) {
                if !buffer.is_null() {
                    (self.api.destroy_tensor_buffer)(buffer);
                }
            }
        }
        result
    }

    fn create_buffer(&self, tensor_type: &LiteRtRankedTensorType) -> Result<Handle, LiteRtRuntimeError> {
        ensure_float32(tensor_type)?;
        let size = tensor_elements(tensor_type)?
            .checked_mul(size_of::<f32>())
            .ok_or_else(|| LiteRtRuntimeError::InvalidTensor("tensor byte size overflow".into()))?;
        let mut buffer = std::ptr::null_mut();
        check(
            "create managed tensor buffer",
            unsafe {
                (self.api.create_managed_tensor_buffer)(
                    self.environment,
                    TENSOR_BUFFER_HOST_MEMORY,
                    tensor_type,
                    size,
                    &mut buffer,
                )
            },
        )?;
        Ok(buffer)
    }

    fn write_buffer(&self, buffer: Handle, values: &[f32]) -> Result<(), LiteRtRuntimeError> {
        let mut host = std::ptr::null_mut();
        check(
            "lock input tensor buffer",
            unsafe { (self.api.lock_tensor_buffer)(buffer, &mut host, TENSOR_BUFFER_LOCK_WRITE) },
        )?;
        if host.is_null() {
            let _ = check("unlock input tensor buffer", unsafe {
                (self.api.unlock_tensor_buffer)(buffer)
            });
            return Err(LiteRtRuntimeError::InvalidTensor(
                "LiteRT returned a null host input buffer".into(),
            ));
        }
        unsafe {
            std::ptr::copy_nonoverlapping(values.as_ptr(), host.cast::<f32>(), values.len());
        }
        check(
            "unlock input tensor buffer",
            unsafe { (self.api.unlock_tensor_buffer)(buffer) },
        )
    }

    fn read_buffer(&self, buffer: Handle, length: usize) -> Result<Vec<f32>, LiteRtRuntimeError> {
        let mut host = std::ptr::null_mut();
        check(
            "lock output tensor buffer",
            unsafe { (self.api.lock_tensor_buffer)(buffer, &mut host, TENSOR_BUFFER_LOCK_READ) },
        )?;
        if host.is_null() {
            let _ = check("unlock output tensor buffer", unsafe {
                (self.api.unlock_tensor_buffer)(buffer)
            });
            return Err(LiteRtRuntimeError::InvalidTensor(
                "LiteRT returned a null host output buffer".into(),
            ));
        }
        let mut values = vec![0.0f32; length];
        unsafe {
            std::ptr::copy_nonoverlapping(host.cast::<f32>(), values.as_mut_ptr(), length);
        }
        check(
            "unlock output tensor buffer",
            unsafe { (self.api.unlock_tensor_buffer)(buffer) },
        )?;
        Ok(values)
    }
}

impl Drop for LiteRtSession {
    fn drop(&mut self) {
        unsafe {
            if !self.compiled_model.is_null() {
                (self.api.destroy_compiled_model)(self.compiled_model);
            }
            if !self.options.is_null() {
                (self.api.destroy_options)(self.options);
            }
            if !self.model.is_null() {
                (self.api.destroy_model)(self.model);
            }
            if !self.environment.is_null() {
                (self.api.destroy_environment)(self.environment);
            }
        }
    }
}

fn collect_tensor_types(
    api: &LiteRtApi,
    model: Handle,
) -> Result<(Vec<LiteRtRankedTensorType>, Vec<LiteRtRankedTensorType>), LiteRtRuntimeError> {
    let mut subgraph_index = 0usize;
    check(
        "get main model subgraph index",
        unsafe { (api.get_main_model_subgraph_index)(model, &mut subgraph_index) },
    )?;
    let mut subgraph = std::ptr::null_mut();
    check(
        "get main model subgraph",
        unsafe { (api.get_model_subgraph)(model, subgraph_index, &mut subgraph) },
    )?;

    let mut input_count = 0usize;
    check(
        "get subgraph input count",
        unsafe { (api.get_num_subgraph_inputs)(subgraph, &mut input_count) },
    )?;
    let mut output_count = 0usize;
    check(
        "get subgraph output count",
        unsafe { (api.get_num_subgraph_outputs)(subgraph, &mut output_count) },
    )?;

    let mut inputs = Vec::with_capacity(input_count);
    for index in 0..input_count {
        let mut tensor = std::ptr::null_mut();
        check(
            "get subgraph input tensor",
            unsafe { (api.get_subgraph_input)(subgraph, index, &mut tensor) },
        )?;
        inputs.push(ranked_tensor_type(api, tensor)?);
    }

    let mut outputs = Vec::with_capacity(output_count);
    for index in 0..output_count {
        let mut tensor = std::ptr::null_mut();
        check(
            "get subgraph output tensor",
            unsafe { (api.get_subgraph_output)(subgraph, index, &mut tensor) },
        )?;
        outputs.push(ranked_tensor_type(api, tensor)?);
    }
    Ok((inputs, outputs))
}

fn ranked_tensor_type(
    api: &LiteRtApi,
    tensor: Handle,
) -> Result<LiteRtRankedTensorType, LiteRtRuntimeError> {
    let mut tensor_type = LiteRtRankedTensorType::default();
    check(
        "get ranked tensor type",
        unsafe { (api.get_ranked_tensor_type)(tensor, &mut tensor_type) },
    )?;
    ensure_float32(&tensor_type)?;
    let _ = dimensions(&tensor_type)?;
    Ok(tensor_type)
}

fn ensure_float32(tensor_type: &LiteRtRankedTensorType) -> Result<(), LiteRtRuntimeError> {
    if tensor_type.element_type != ELEMENT_TYPE_FLOAT32 {
        return Err(LiteRtRuntimeError::InvalidTensor(format!(
            "expected float32 tensor, element type is {}",
            tensor_type.element_type
        )));
    }
    Ok(())
}

fn tensor_elements(tensor_type: &LiteRtRankedTensorType) -> Result<usize, LiteRtRuntimeError> {
    dimensions(tensor_type)?.iter().try_fold(1usize, |total, dimension| {
        total
            .checked_mul(*dimension as usize)
            .ok_or_else(|| LiteRtRuntimeError::InvalidTensor("tensor element count overflow".into()))
    })
}

fn dimensions(tensor_type: &LiteRtRankedTensorType) -> Result<Vec<i32>, LiteRtRuntimeError> {
    tensor_type.layout.dimensions()
}

fn check(operation: &'static str, status: Status) -> Result<(), LiteRtRuntimeError> {
    if status == STATUS_OK {
        Ok(())
    } else {
        Err(LiteRtRuntimeError::Status { operation, status })
    }
}

fn runtime_filename() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "libLiteRt.dll"
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        "libLiteRt.so"
    }
    #[cfg(target_os = "macos")]
    {
        "libLiteRt.dylib"
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "android",
        target_os = "macos"
    )))]
    {
        "libLiteRt"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_abi_layout_matches_litert_public_header() {
        assert_eq!(size_of::<LiteRtLayout>(), 68);
        assert_eq!(size_of::<LiteRtRankedTensorType>(), 72);
    }

    #[test]
    fn layout_rank_and_dimensions_decode_public_bitfield_layout() {
        let mut layout = LiteRtLayout::default();
        layout.flags = 4;
        layout.dimensions[..4].copy_from_slice(&[1, 256, 256, 3]);
        assert_eq!(layout.rank(), 4);
        assert_eq!(layout.dimensions().unwrap(), vec![1, 256, 256, 3]);
    }
}
