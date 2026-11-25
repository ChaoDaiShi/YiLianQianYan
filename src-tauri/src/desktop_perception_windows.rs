use image::{
    codecs::png::PngEncoder, imageops::FilterType, ExtendedColorType, ImageEncoder, RgbaImage,
};
use std::{slice, time::Duration};
use tauri::Manager;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use windows::{
    core::{IInspectable, Interface},
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{
            Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
            GraphicsCapturePicker, GraphicsCaptureSession,
        },
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
    },
    Win32::{
        Foundation::{E_ABORT, E_POINTER},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
                D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            },
            Dxgi::IDXGIDevice,
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED,
        },
        UI::Shell::IInitializeWithWindow,
    },
};
use windows_future::{AsyncOperationCompletedHandler, IAsyncOperation};

pub(super) async fn pick(
    view: tauri::Webview,
    token: &CancellationToken,
) -> Result<Option<GraphicsCaptureItem>, String> {
    let app = view.app_handle().clone();
    let (started, start) = oneshot::channel();
    let (finished, finish) = oneshot::channel();
    app.run_on_main_thread(move || {
        let result = (|| -> Result<IAsyncOperation<GraphicsCaptureItem>, String> {
            if !GraphicsCaptureSession::IsSupported().map_err(message)? {
                return Err("screen_capture_unsupported".into());
            }
            let picker = GraphicsCapturePicker::new().map_err(message)?;
            let owner = view.window().hwnd().map_err(|error| error.to_string())?;
            let initializer = picker.cast::<IInitializeWithWindow>().map_err(message)?;
            // The owner window and picker are created on the same UI thread.
            unsafe { initializer.Initialize(owner) }.map_err(message)?;
            let operation = picker.PickSingleItemAsync().map_err(message)?;
            let mut finished = Some(finished);
            operation
                .SetCompleted(&AsyncOperationCompletedHandler::new(move |operation, _| {
                    let result = match operation
                        .as_ref()
                        .ok_or(E_POINTER.into())
                        .and_then(|op| op.GetResults())
                    {
                        Ok(target) => Ok(Some(target)),
                        Err(error) if matches!(error.code(), E_ABORT | E_POINTER) => Ok(None),
                        Err(error) => Err(message(error)),
                    };
                    if let Some(finished) = finished.take() {
                        let _ = finished.send(result);
                    }
                    Ok(())
                }))
                .map_err(message)?;
            Ok(operation)
        })();
        let _ = started.send(result);
    })
    .map_err(|error| error.to_string())?;
    let operation = start.await.map_err(|_| "screen_picker_interrupted")??;
    tokio::select! {
        biased;
        _ = token.cancelled() => {
            let _ = app.run_on_main_thread(move || { let _ = operation.Cancel(); });
            Ok(None)
        }
        chosen = finish => chosen.map_err(|_| "screen_picker_interrupted".to_owned())?,
    }
}

pub(super) async fn snapshot(
    target: GraphicsCaptureItem,
    token: CancellationToken,
) -> Result<Vec<u8>, String> {
    // The session and D3D device are owned by one MTA worker. The callback only
    // signals that a frame exists, and the worker closes capture before encoding.
    tokio::task::spawn_blocking(move || {
        let _apartment = Apartment::enter().map_err(message)?;
        let (device, winrt) = devices().map_err(message)?;
        let size = target.Size().map_err(message)?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            1,
            size,
        )
        .map_err(message)?;
        let pool = CloseCapture(pool);
        let session = CloseCapture(pool.0.CreateCaptureSession(&target).map_err(message)?);
        let (tx, mut rx) = mpsc::channel(1);
        pool.0
            .FrameArrived(&TypedEventHandler::<_, IInspectable>::new(move |_, _| {
                let _ = tx.try_send(());
                Ok(())
            }))
            .map_err(message)?;
        if token.is_cancelled() {
            return Err("screen_permission_revoked".into());
        }
        session.0.StartCapture().map_err(message)?;
        tokio::runtime::Handle::current().block_on(async {
            tokio::select! {
                biased;
                _ = token.cancelled() => Err("screen_permission_revoked"),
                result = tokio::time::timeout(Duration::from_secs(3), rx.recv()) =>
                    result.ok().flatten().ok_or("screen_frame_timeout"),
            }
        })?;
        let frame = CloseCapture(pool.0.TryGetNextFrame().map_err(message)?);
        let pixels = copy_frame(&device, &frame.0).map_err(message)?;
        drop(frame);
        drop(session);
        drop(pool);
        if token.is_cancelled() {
            return Err("screen_permission_revoked".into());
        }
        encode_frame(pixels)
    })
    .await
    .map_err(|_| "screen_capture_interrupted".to_owned())?
}

fn message(error: windows::core::Error) -> String {
    format!("screen_capture_failed ({})", error.code())
}

struct Apartment;

impl Apartment {
    fn enter() -> windows::core::Result<Self> {
        // The blocking task has no UI apartment.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
        Ok(Self)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

struct CloseCapture<T: Interface>(T);

impl<T: Interface> Drop for CloseCapture<T> {
    fn drop(&mut self) {
        if let Ok(closable) = self.0.cast::<windows::Foundation::IClosable>() {
            let _ = closable.Close();
        }
    }
}

fn devices() -> windows::core::Result<(ID3D11Device, IDirect3DDevice)> {
    let mut native = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            Default::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut native),
            None,
            None,
        )?;
        let native = native.ok_or(E_POINTER)?;
        let winrt = CreateDirect3D11DeviceFromDXGIDevice(&native.cast::<IDXGIDevice>()?)?
            .cast::<IDirect3DDevice>()?;
        Ok((native, winrt))
    }
}

fn copy_frame(
    device: &ID3D11Device,
    frame: &Direct3D11CaptureFrame,
) -> windows::core::Result<RgbaImage> {
    let size = frame.ContentSize()?;
    let access = frame.Surface()?.cast::<IDirect3DDxgiInterfaceAccess>()?;
    // Map a CPU staging texture instead of accessing graphics-owned memory.
    unsafe {
        let source: ID3D11Texture2D = access.GetInterface()?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        source.GetDesc(&mut desc);
        let width = desc.Width.min(size.Width.max(0) as u32);
        let height = desc.Height.min(size.Height.max(0) as u32);
        if width == 0 || height == 0 {
            return Err(E_POINTER.into());
        }
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging = None;
        device.CreateTexture2D(&desc, None, Some(&mut staging))?;
        let staging = staging.ok_or(E_POINTER)?;
        let context = device.GetImmediateContext()?;
        context.CopyResource(&staging, &source);
        let mut data = D3D11_MAPPED_SUBRESOURCE::default();
        context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut data))?;
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for row in 0..height as usize {
            let row = slice::from_raw_parts(
                data.pData.cast::<u8>().add(row * data.RowPitch as usize),
                width as usize * 4,
            );
            for bgra in row.chunks_exact(4) {
                pixels.extend_from_slice(&[bgra[2], bgra[1], bgra[0], 255]);
            }
        }
        context.Unmap(&staging, 0);
        RgbaImage::from_raw(width, height, pixels).ok_or_else(|| E_POINTER.into())
    }
}

fn encode_frame(frame: RgbaImage) -> Result<Vec<u8>, String> {
    let longest = frame.width().max(frame.height());
    let frame = if longest > 1920 {
        image::imageops::resize(
            &frame,
            (frame.width() * 1920 / longest).max(1),
            (frame.height() * 1920 / longest).max(1),
            FilterType::Triangle,
        )
    } else {
        frame
    };
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(
            frame.as_raw(),
            frame.width(),
            frame.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|_| "screen_encode_failed".to_owned())?;
    Ok(png)
}
