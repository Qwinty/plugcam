//! "Plugcam Camera": the DirectShow virtual camera from our Softcam fork (`vcam/`).
//!
//! The DLL is loaded at runtime so the same binary works whether it sits next to the
//! exe (installed app) or in `vcam/dist` (development). Registration with regsvr32 is
//! done by the installer; this module only feeds frames through the sender API.

use std::ffi::c_void;
use std::path::Path;

use libloading::Library;

type CreateCamera = unsafe extern "C" fn(width: i32, height: i32, framerate: f32) -> *mut c_void;
type DeleteCamera = unsafe extern "C" fn(camera: *mut c_void);
type SendFrame = unsafe extern "C" fn(camera: *mut c_void, image_bits: *const c_void);
type IsConnected = unsafe extern "C" fn(camera: *mut c_void) -> bool;

#[derive(Debug, thiserror::Error)]
pub enum VcamError {
    #[error("cannot load {path}: {source}")]
    Load { path: String, source: libloading::Error },
    #[error("width and height must be positive multiples of 4, got {0}x{1}")]
    BadSize(u32, u32),
    #[error("scCreateCamera failed (is another Plugcam instance already running?)")]
    CreateFailed,
}

/// A live virtual camera. Frames are 24-bit BGR, rows top to bottom, no padding.
/// The camera disappears for client apps when this is dropped.
pub struct VirtualCamera {
    handle: *mut c_void,
    width: u32,
    height: u32,
    delete: DeleteCamera,
    send: SendFrame,
    is_connected: IsConnected,
    // Keeps the function pointers above valid; must outlive `handle`.
    _lib: Library,
}

// The sender API has no thread affinity; we only ever use a camera from one thread at a time.
unsafe impl Send for VirtualCamera {}

impl VirtualCamera {
    /// Creates the camera with a fixed size. Framerate 0 means "every frame is pushed
    /// out as soon as `send_frame` is called", which is what a live source wants.
    pub fn create(dll: &Path, width: u32, height: u32) -> Result<Self, VcamError> {
        if width == 0 || height == 0 || width % 4 != 0 || height % 4 != 0 {
            return Err(VcamError::BadSize(width, height));
        }
        let load_err = |source| VcamError::Load { path: dll.display().to_string(), source };
        unsafe {
            let lib = Library::new(dll).map_err(load_err)?;
            let create = *lib.get::<CreateCamera>(b"scCreateCamera\0").map_err(load_err)?;
            let delete = *lib.get::<DeleteCamera>(b"scDeleteCamera\0").map_err(load_err)?;
            let send = *lib.get::<SendFrame>(b"scSendFrame\0").map_err(load_err)?;
            let is_connected = *lib.get::<IsConnected>(b"scIsConnected\0").map_err(load_err)?;

            let handle = create(width as i32, height as i32, 0.0);
            if handle.is_null() {
                return Err(VcamError::CreateFailed);
            }
            Ok(Self { handle, width, height, delete, send, is_connected, _lib: lib })
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Byte length `send_frame` expects.
    pub fn frame_len(&self) -> usize {
        self.width as usize * self.height as usize * 3
    }

    /// Publishes one BGR frame. Panics if the buffer size does not match the camera.
    pub fn send_frame(&self, bgr: &[u8]) {
        assert_eq!(bgr.len(), self.frame_len(), "frame size does not match the camera");
        unsafe { (self.send)(self.handle, bgr.as_ptr().cast()) }
    }

    /// True while some application has the camera open.
    pub fn is_connected(&self) -> bool {
        unsafe { (self.is_connected)(self.handle) }
    }
}

impl Drop for VirtualCamera {
    fn drop(&mut self) {
        unsafe { (self.delete)(self.handle) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_sizes_softcam_cannot_handle() {
        for (w, h) in [(0, 1080), (1920, 0), (1922, 1080), (1920, 1082)] {
            let err = VirtualCamera::create(Path::new("unused.dll"), w, h).err().unwrap();
            assert!(matches!(err, VcamError::BadSize(..)), "{w}x{h}: {err}");
        }
    }
}
