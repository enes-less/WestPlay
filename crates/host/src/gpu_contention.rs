//! Measurement (tests only): a page drawing on the graphics card while a game keeps the card busy, and whether a
//! higher GPU scheduling priority for WestPlay's browser keeps its frames coming, at what cost to the game. A
//! player's log (RTX 4060, 2026-11-09): the cab page at 5-15 frames a second while the game ran 40-57 and the page
//! used almost no processor.
//!
//! The game stand-in draws a costly full-screen pass, three frames queued as a game's driver queues them, tuned to
//! about 25 ms a frame; the page stand-in, in an offscreen WebView as the cab's, draws many triangles with
//! antialiasing every animation frame (as the map does), counting its frames. Both on one GPU: the default, or the
//! one named in `WESTPLAY_CONTENTION_GPU`.
//!
//! Measured 2026-11-09: on the RTX 4080 no contention at all (the page 75 of 75 Hz whatever the priority, the game
//! 34). On the 7950X's Radeon (`WESTPLAY_CONTENTION_GPU=radeon`): the page 32 alone, 12 beside the game (the
//! player's 5-15); the browser's GPU priority above normal 12 (the game 22 of 23), high 14 (the game 20): no fix.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use windows::Wdk::Graphics::Direct3D::{
    D3DKMT_SCHEDULINGPRIORITYCLASS, D3DKMT_SCHEDULINGPRIORITYCLASS_ABOVE_NORMAL, D3DKMT_SCHEDULINGPRIORITYCLASS_HIGH,
    D3DKMT_SCHEDULINGPRIORITYCLASS_NORMAL, D3DKMTSetProcessSchedulingPriorityClass,
};
use windows::Win32::Foundation::{CloseHandle, HMODULE};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST, ID3DBlob,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_RENDER_TARGET, D3D11_BUFFER_DESC, D3D11_CREATE_DEVICE_FLAG,
    D3D11_QUERY_DESC, D3D11_QUERY_EVENT, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_VIEWPORT,
    D3D11CreateDevice, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader, ID3D11Query,
    ID3D11RenderTargetView, ID3D11Texture2D, ID3D11VertexShader,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION};
use windows::core::{Interface, PCSTR, s};

use crate::webview::{OffscreenWebView, Stage};

const VS: &str = "float4 main(uint id : SV_VertexID) : SV_Position {
    float2 p = float2((id << 1) & 2, id & 2);
    return float4(p * float2(2, -2) + float2(-1, 1), 0, 1);
}";

const PS: &str = "cbuffer Work : register(b0) { uint steps; uint3 pad; };
float4 main(float4 at : SV_Position) : SV_Target {
    float v = at.x * 0.001 + at.y * 0.002;
    for (uint i = 0; i < steps; i++) { v = frac(sin(v * 12.9898 + i) * 43758.5453); }
    return float4(v, v * 0.5, 1 - v, 1);
}";

fn compile(source: &str, entry: PCSTR, target: PCSTR) -> ID3DBlob {
    let mut code = None;
    let mut errors = None;
    // SAFETY: the source lives through the call; the blobs are written into locals.
    let compiled = unsafe {
        D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            None,
            None,
            None,
            entry,
            target,
            0,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    if compiled.is_err() {
        // SAFETY: the error blob's own buffer, alive while it is.
        let message = errors.map(|e| unsafe {
            String::from_utf8_lossy(std::slice::from_raw_parts(e.GetBufferPointer().cast::<u8>(), e.GetBufferSize()))
                .into_owned()
        });
        panic!("shader: {message:?}");
    }
    code.unwrap()
}

fn bytes(blob: &ID3DBlob) -> &[u8] {
    // SAFETY: the blob's own buffer, alive while it is.
    unsafe { std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize()) }
}

/// The game stand-in: a full-screen pass of `steps` per pixel at 1920x1080, one a frame, each frame waited for (as
/// a game's frames follow one another), counting frames.
struct Game {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    target: ID3D11RenderTargetView,
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    work: ID3D11Buffer,
    /// A query a frame, three of them: a frame waits only for the one three before (as a game's frames queue).
    done: [ID3D11Query; 3],
    next: std::cell::Cell<usize>,
}

/// The adapter whose name has `named` in it (`WESTPLAY_CONTENTION_GPU`), and its LUID; none: the default.
fn adapter(named: Option<&str>) -> Option<(IDXGIAdapter1, windows::Win32::Foundation::LUID, String)> {
    let named = named?.to_lowercase();
    // SAFETY: plain DXGI calls; the descriptions are locals.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut index = 0;
        while let Ok(adapter) = factory.EnumAdapters1(index) {
            let desc = adapter.GetDesc1().ok()?;
            let name = String::from_utf16_lossy(&desc.Description).trim_end_matches('\0').to_owned();
            if name.to_lowercase().contains(&named) {
                return Some((adapter, desc.AdapterLuid, name));
            }
            index += 1;
        }
    }
    None
}

impl Game {
    fn new(on: Option<IDXGIAdapter1>) -> Game {
        let (mut device, mut context) = (None, None);
        let kind = if on.is_some() { D3D_DRIVER_TYPE_UNKNOWN } else { D3D_DRIVER_TYPE_HARDWARE };
        // SAFETY: plain Direct3D 11 calls on the test's own device; every description is a local.
        unsafe {
            D3D11CreateDevice(
                on.map(|adapter| adapter.cast::<windows::Win32::Graphics::Dxgi::IDXGIAdapter>().unwrap()).as_ref(),
                kind,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .unwrap();
            let (device, context): (ID3D11Device, ID3D11DeviceContext) = (device.unwrap(), context.unwrap());
            let desc = D3D11_TEXTURE2D_DESC {
                Width: 1920,
                Height: 1080,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut texture: Option<ID3D11Texture2D> = None;
            device.CreateTexture2D(&desc, None, Some(&mut texture)).unwrap();
            let mut target = None;
            device.CreateRenderTargetView(&texture.unwrap(), None, Some(&mut target)).unwrap();
            let vs_code = compile(VS, s!("main"), s!("vs_5_0"));
            let ps_code = compile(PS, s!("main"), s!("ps_5_0"));
            let (mut vs, mut ps) = (None, None);
            device.CreateVertexShader(bytes(&vs_code), None, Some(&mut vs)).unwrap();
            device.CreatePixelShader(bytes(&ps_code), None, Some(&mut ps)).unwrap();
            let mut work = None;
            let buffer = D3D11_BUFFER_DESC {
                ByteWidth: 16,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                ..Default::default()
            };
            device.CreateBuffer(&buffer, None, Some(&mut work)).unwrap();
            let query = || {
                let mut done = None;
                device
                    .CreateQuery(&D3D11_QUERY_DESC { Query: D3D11_QUERY_EVENT, MiscFlags: 0 }, Some(&mut done))
                    .unwrap();
                done.unwrap()
            };
            let done = [query(), query(), query()];
            Game {
                device,
                context,
                target: target.unwrap(),
                vs: vs.unwrap(),
                ps: ps.unwrap(),
                work: work.unwrap(),
                done,
                next: std::cell::Cell::new(0),
            }
        }
    }

    /// One frame of `steps` per pixel, waited for (`queued`: only the one three before it); its time.
    fn frame(&self, steps: u32, queued: bool) -> Duration {
        let started = Instant::now();
        // SAFETY: the test's own device and resources; the constant is a local copied in.
        unsafe {
            let work = [steps, 0, 0, 0];
            self.context.UpdateSubresource(&self.work, 0, None, work.as_ptr().cast(), 0, 0);
            self.context.OMSetRenderTargets(Some(&[Some(self.target.clone())]), None);
            self.context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: 1920.0,
                Height: 1080.0,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            self.context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.context.VSSetShader(&self.vs, None);
            self.context.PSSetShader(&self.ps, None);
            self.context.PSSetConstantBuffers(0, Some(&[Some(self.work.clone())]));
            self.context.Draw(3, 0);
            let at = self.next.get();
            self.next.set((at + 1) % 3);
            self.context.End(&self.done[at]);
            self.context.Flush();
            // This frame's (`queued`: the oldest of three, ending now its turn has come round again).
            let wait = if queued { (at + 1) % 3 } else { at };
            let mut finished = 0u32;
            while self.context.GetData(&self.done[wait], Some((&mut finished as *mut u32).cast()), 4, 0) != Ok(())
                || finished == 0
            {
                std::thread::sleep(Duration::from_micros(200));
            }
        }
        let _ = &self.device;
        started.elapsed()
    }
}

/// The page stand-in: many triangles drawn with antialiasing every animation frame, its frames counted
/// (`window.frames`).
const PAGE: &str = r#"<!doctype html><body style="margin:0;background:#111"><canvas id=c width=1040 height=560></canvas><script>
const gl = document.getElementById('c').getContext('webgl', { antialias: true });
const vs = gl.createShader(gl.VERTEX_SHADER);
gl.shaderSource(vs, 'attribute vec2 p; uniform float t; void main() { gl_Position = vec4(p.x + 0.02 * sin(t + p.y * 9.0), p.y, 0.0, 1.0); }');
gl.compileShader(vs);
const fs = gl.createShader(gl.FRAGMENT_SHADER);
gl.shaderSource(fs, 'precision mediump float; void main() { gl_FragColor = vec4(0.3, 0.6, 0.9, 0.5); }');
gl.compileShader(fs);
const prog = gl.createProgram(); gl.attachShader(prog, vs); gl.attachShader(prog, fs); gl.linkProgram(prog); gl.useProgram(prog);
const n = 60000; const data = new Float32Array(n * 6);
for (let i = 0; i < n; i++) { const x = Math.random() * 2 - 1, y = Math.random() * 2 - 1;
  data.set([x, y, x + 0.03, y, x, y + 0.05], i * 6); }
const buf = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, buf); gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
const at = gl.getAttribLocation(prog, 'p'); gl.enableVertexAttribArray(at); gl.vertexAttribPointer(at, 2, gl.FLOAT, false, 0, 0);
const t = gl.getUniformLocation(prog, 't');
gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
window.frames = 0;
function draw(now) { window.frames++; gl.clearColor(0.05, 0.05, 0.06, 1); gl.clear(gl.COLOR_BUFFER_BIT);
  gl.uniform1f(t, now / 500); for (let k = 0; k < 4; k++) gl.drawArrays(gl.TRIANGLES, 0, n * 3); requestAnimationFrame(draw); }
requestAnimationFrame(draw);
</script>"#;

/// The page's frames counted so far.
fn page_frames(view: &OffscreenWebView) -> u64 {
    view.evaluate("window.frames").ok().and_then(|frames| frames.parse().ok()).unwrap_or(0)
}

/// Pumps the thread's messages for `seconds` (the page's work arrives through them).
fn pump(seconds: f64) {
    use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage};
    let until = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < until {
        let mut msg = MSG::default();
        // SAFETY: the thread's own message queue.
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Every WebView2 process of this test's browsers (their browser and everything under it).
fn browser_processes() -> Vec<u32> {
    let mut tree: Vec<(u32, u32)> = Vec::new();
    // SAFETY: a snapshot of the process list, walked with an entry sized as the calls require, then closed.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).unwrap();
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            tree.push((entry.th32ProcessID, entry.th32ParentProcessID));
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    let mut found: Vec<u32> = crate::webview::host_browsers();
    let mut at = 0;
    while at < found.len() {
        let parent = found[at];
        for &(pid, of) in &tree {
            if of == parent && !found.contains(&pid) {
                found.push(pid);
            }
        }
        at += 1;
    }
    found
}

/// Sets every one of `pids`' GPU scheduling priority; how many took it.
fn set_priority(pids: &[u32], class: D3DKMT_SCHEDULINGPRIORITYCLASS) -> usize {
    pids.iter()
        .filter(|&&pid| {
            // SAFETY: a handle opened to set the process's GPU priority, closed after.
            unsafe {
                let Ok(process) = OpenProcess(PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                else {
                    return false;
                };
                let status = D3DKMTSetProcessSchedulingPriorityClass(process, class);
                let _ = CloseHandle(process);
                status.is_ok()
            }
        })
        .count()
}

#[test]
#[ignore = "measures this PC's graphics card for a minute or so"]
fn a_page_beside_a_game_that_keeps_the_card_busy() {
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
    // SAFETY: COM on this test's own thread, once, before anything uses it.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };

    // Both on one GPU: the one named in `WESTPLAY_CONTENTION_GPU` (`radeon`: a weak one), else the default.
    let named = std::env::var("WESTPLAY_CONTENTION_GPU").ok();
    let chosen = adapter(named.as_deref());
    if let Some((_, luid, name)) = &chosen {
        println!("on {name}");
        crate::webview::draw_on(Some(crate::graphics::browser_argument(*luid)));
    }
    let luid = chosen.as_ref().map(|(_, luid, _)| *luid);
    // The game stand-in, on its own thread: tuned alone to about 40 frames a second, then run on demand.
    let running = Arc::new(AtomicBool::new(false));
    let frames = Arc::new(AtomicU64::new(0));
    let steps = Arc::new(AtomicU32::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let game = {
        let (running, frames, steps, stop) = (running.clone(), frames.clone(), steps.clone(), stop.clone());
        std::thread::spawn(move || {
            // Its own adapter object for its thread, found again by its LUID.
            let on = luid.and_then(|luid| {
                // SAFETY: plain DXGI calls.
                unsafe {
                    let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
                    let mut index = 0;
                    while let Ok(adapter) = factory.EnumAdapters1(index) {
                        let desc = adapter.GetDesc1().ok()?;
                        if desc.AdapterLuid.LowPart == luid.LowPart && desc.AdapterLuid.HighPart == luid.HighPart {
                            return Some(adapter);
                        }
                        index += 1;
                    }
                    None
                }
            });
            let game = Game::new(on);
            // Tuned: the work per pixel that takes about 25 ms a frame.
            let mut work = 64u32;
            loop {
                let took = (0..3).map(|_| game.frame(work, false)).min().unwrap();
                if took >= Duration::from_millis(25) || work > 1 << 20 {
                    break;
                }
                work = (f64::from(work) * (25.0 / took.as_secs_f64().max(0.0005) / 1000.0).clamp(1.2, 8.0)) as u32;
            }
            steps.store(work, Ordering::Release);
            while !stop.load(Ordering::Acquire) {
                if running.load(Ordering::Acquire) {
                    game.frame(work, true);
                    frames.fetch_add(1, Ordering::Relaxed);
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        })
    };
    while steps.load(Ordering::Acquire) == 0 {
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("the game stand-in: {} steps a pixel for about 25 ms a frame", steps.load(Ordering::Acquire));

    let root = std::env::temp_dir().join(format!("westplay-contention-{}", std::process::id()));
    let stage = Stage::new(1040, 560).unwrap();
    let view = OffscreenWebView::new(&stage, 1040, 560, &root.join("profile")).unwrap();
    stage.add(&view, false).unwrap();
    view.navigate_to_string(PAGE).unwrap();
    pump(3.0);

    // Each phase: 6 s; the page's frames and the game's over it.
    let measure = |name: &str, with_game: bool| {
        running.store(with_game, Ordering::Release);
        pump(1.0);
        let (page0, game0, at) = (page_frames(&view), frames.load(Ordering::Relaxed), Instant::now());
        pump(6.0);
        let seconds = at.elapsed().as_secs_f64();
        let page = (page_frames(&view) - page0) as f64 / seconds;
        let game_fps = (frames.load(Ordering::Relaxed) - game0) as f64 / seconds;
        println!("{name}: the page {page:.0} frames a second, the game {game_fps:.0}");
        running.store(false, Ordering::Release);
    };
    measure("the page alone", false);
    let game_alone = {
        running.store(true, Ordering::Release);
        let start = frames.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_secs(4));
        running.store(false, Ordering::Release);
        (frames.load(Ordering::Relaxed) - start) as f64 / 4.0
    };
    println!("the game alone: {game_alone:.0} frames a second");
    measure("both, the browser's GPU priority normal", true);
    let browsers = browser_processes();
    for (name, class) in [
        ("above normal", D3DKMT_SCHEDULINGPRIORITYCLASS_ABOVE_NORMAL),
        ("high", D3DKMT_SCHEDULINGPRIORITYCLASS_HIGH),
        ("normal again", D3DKMT_SCHEDULINGPRIORITYCLASS_NORMAL),
    ] {
        let took = set_priority(&browsers, class);
        measure(
            &format!("both, the browser's GPU priority {name} ({took} of {} processes took it)", browsers.len()),
            true,
        );
    }
    stop.store(true, Ordering::Release);
    game.join().unwrap();
}
