use core::{f32, panic};
use std::cell::SyncUnsafeCell;
use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Instant;

use crate::scoped_threadpool::Pool;
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::particles::Particles;
use std::thread::available_parallelism;

const TARGET_FRAMETIME: f32 = 20.0;
const N_INITIAL_PARTICELS: usize = 1_000;

struct AppData<'a> {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    particles: Particles<'a>,
    count_buffer: Vec<AtomicU16>,
}

struct App<'a> {
    data: Option<AppData<'a>>,
    last_frametime: Instant,
    frametime_buffer: VecDeque<f32>,
    n_frame: u32,
    threadpool: &'a Pool,
    mouse_pos: (f32, f32),
    mouse_down: bool,
}

impl<'a> App<'a> {
    fn new(threadpool: &'a Pool) -> Self {
        App {
            data: None,
            n_frame: 0,
            last_frametime: Instant::now(),
            frametime_buffer: VecDeque::new(),
            threadpool,
            mouse_pos: (0.0, 0.0),
            mouse_down: false,
        }
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Rc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );
        let context = Context::new(Rc::clone(&window)).unwrap();
        let surface = softbuffer::Surface::new(&context, Rc::clone(&window)).unwrap();
        let particles = Particles::new(self.threadpool);
        self.data = Some(AppData {
            surface,
            window,
            particles,
            count_buffer: Vec::new(),
        })
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let _ = id;
        let Some(data) = &mut self.data else {
            panic!();
        };
        event_loop.set_control_flow(ControlFlow::Poll);

        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.frametime_buffer.clear();
                data.particles
                    .reset(N_INITIAL_PARTICELS, size.width, size.height);
                let buffer_size = (size.width * size.height) as usize;
                data.count_buffer.clear();
                data.count_buffer.reserve(buffer_size);
                (0..buffer_size).for_each(|_| data.count_buffer.push(AtomicU16::new(0)));
            }
            WindowEvent::CursorMoved {
                device_id: _,
                position,
            } => {
                self.mouse_pos = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput {
                device_id: _,
                state,
                button: MouseButton::Left,
            } => {
                self.mouse_down = state == ElementState::Pressed;
            }
            WindowEvent::RedrawRequested => {
                let (width, height) = {
                    let size = data.window.inner_size();
                    (size.width, size.height)
                };

                self.n_frame += 1;
                let now = Instant::now();
                let frametime = now.duration_since(self.last_frametime);
                self.last_frametime = now;

                if self.frametime_buffer.len() > 100 {
                    self.frametime_buffer.pop_back();
                }
                self.frametime_buffer.push_front(frametime.as_millis_f32());
                let frametime_avg =
                    self.frametime_buffer.iter().sum::<f32>() / self.frametime_buffer.len() as f32;

                if self.n_frame % 100 == 0 {
                    println!("#{}: FPS = {}", self.n_frame, 1000.0 / frametime_avg);
                    println!("n_particles = {}", data.particles.particles.len());
                }

                let frametime_ratio = TARGET_FRAMETIME / frametime_avg.clamp(10.0, 100.0);
                if frametime_ratio > 1.1 {
                    let n = data.particles.particles.len() as f32 * (frametime_ratio - 1.0) / 3.0;
                    data.particles.add_particles(n as usize, width, height);
                } else if frametime_ratio < 0.9 {
                    let n = data.particles.particles.len() as f32 * (1.0 - frametime_ratio) / 200.0;
                    for _ in 0..n as usize {
                        data.particles.particles.pop();
                    }
                }

                data.particles
                    .update(&frametime, self.mouse_pos, self.mouse_down);

                data.count_buffer.iter().for_each(|count| {
                    count.store(0, Ordering::Relaxed);
                });

                let particles_chunk_len = usize::max(
                    data.particles.particles.len() / self.threadpool.thread_count() as usize / 10,
                    1,
                );

                let particles_chunks = data.particles.particles.chunks(particles_chunk_len);

                let count_buffer_ref = &data.count_buffer;

                self.threadpool.scoped(|scope| {
                    for particles_chunk in particles_chunks {
                        scope.execute(move |_| {
                            for particle in particles_chunk {
                                let inside = particle.x >= 0.0
                                    && particle.x < (width as f32 - 1.0)
                                    && particle.y >= 0.0
                                    && particle.y < (height as f32 - 1.0);

                                let x = (particle.x as usize).clamp(0, width as usize - 1);
                                let y = (particle.y as usize).clamp(0, height as usize - 1);

                                count_buffer_ref[x + y * width as usize]
                                    .fetch_add(inside as u16, Ordering::Relaxed);
                            }
                        });
                    }
                });

                data.surface
                    .resize(
                        NonZeroU32::new(width).unwrap(),
                        NonZeroU32::new(height).unwrap(),
                    )
                    .unwrap();

                let mut pixel_buffer = data.surface.buffer_mut().unwrap();

                let pixel_chunk_len =
                    u32::max(width * height / self.threadpool.thread_count() / 10, 1) as usize;

                let pixel_buffer_chunks = pixel_buffer
                    .chunks_exact_mut(pixel_chunk_len)
                    .collect::<Vec<_>>();

                let count_buffer_chunks = data
                    .count_buffer
                    .chunks_exact_mut(pixel_chunk_len)
                    .collect::<Vec<_>>();

                self.threadpool.scoped(|scope| {
                    for (i_chunk, (pixel_buffer_chunk, count_buffer_chunk)) in pixel_buffer_chunks
                        .into_iter()
                        .zip(count_buffer_chunks.into_iter())
                        .enumerate()
                    {
                        scope.execute(move |_| {
                            for (i_pixel, (pixel, count)) in pixel_buffer_chunk
                                .iter_mut()
                                .zip(count_buffer_chunk.iter_mut())
                                .enumerate()
                            {
                                let index = i_chunk * pixel_chunk_len + i_pixel;
                                let x = (index % width as usize) as f32;
                                let y = (index / width as usize) as f32;
                                let count = *count.get_mut() as f32 * 32.0; // for pixel in pixel_buffer_chunk.iter_mut() {
                                let red = (x * count / width as f32) as u8;
                                let green = (y * count / height as f32) as u8;
                                let blue = ((1.0 - (x / width as f32) - (y / height as f32))
                                    * count) as u8;
                                *pixel =
                                    ((red as u32) << 16) + ((green as u32) << 8) + (blue as u32)
                            }
                        });
                    }
                });

                pixel_buffer.present().unwrap();
                data.window.request_redraw();
            }
            _ => (),
        }
    }
}

pub fn run() {
    let event_loop = EventLoop::new().unwrap();

    // ControlFlow::Poll continuously runs the event loop, even if the OS hasn't
    // dispatched any events. This is ideal for games and similar applications.
    event_loop.set_control_flow(ControlFlow::Poll);

    let n_threads = available_parallelism().unwrap().get();
    let threadpool = Pool::new(n_threads);
    let mut app = App::new(&threadpool);
    let _ = event_loop.run_app(&mut app);
}

// unsafe fn make_mutable<T>(reference: &T) -> &mut T {
//     let const_ptr = reference as *const T;
//     let mut_ptr = const_ptr as *mut T;
//     &mut *mut_ptr
// }
