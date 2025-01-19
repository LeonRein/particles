use core::{f32, panic};
use std::cell::SyncUnsafeCell;
use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::rc::Rc;
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
        })
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let _ = id;
        let Some(AppData {
            window,
            surface,
            particles,
        }) = &mut self.data
        else {
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
                particles.reset(N_INITIAL_PARTICELS, size.width, size.height);
                println!("window resized");
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
                    let size = window.inner_size();
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
                    println!("n_particles = {}", particles.particles.len());
                }

                let frametime_ratio = TARGET_FRAMETIME / frametime_avg.clamp(10.0, 100.0);
                if frametime_ratio > 1.1 {
                    let n = particles.particles.len() as f32 * (frametime_ratio - 1.0) / 3.0;
                    particles.add_particles(n as usize, width, height);
                } else if frametime_ratio < 0.9 {
                    let n = particles.particles.len() as f32 * (1.0 - frametime_ratio) / 200.0;
                    for _ in 0..n as usize {
                        particles.particles.pop();
                    }
                }

                particles.update(&frametime, self.mouse_pos, self.mouse_down);

                let particles_chunk_len = usize::max(
                    particles.particles.len() / self.threadpool.thread_count() as usize / 10,
                    1,
                );

                let particles_chunks = particles.particles.chunks(particles_chunk_len);

                let super_count_buffer_cell = (0..self.threadpool.thread_count())
                    .map(|_| vec![0_u16; (width * height) as usize])
                    .map(SyncUnsafeCell::new)
                    .collect::<Vec<_>>();

                let super_count_buffer_cell_ref = &super_count_buffer_cell;

                self.threadpool.scoped(|scope| {
                    for particles_chunk in particles_chunks {
                        scope.execute(move |id| {
                            for particle in particles_chunk {
                                let inside = particle.x >= 0.0
                                    && particle.x < (width as f32 - 1.0)
                                    && particle.y >= 0.0
                                    && particle.y < (height as f32 - 1.0);

                                let x = (particle.x as usize).clamp(0, width as usize - 1);
                                let y = (particle.y as usize).clamp(0, height as usize - 1);

                                unsafe {
                                    let count_buffer = super_count_buffer_cell_ref[id].get();
                                    (*count_buffer)[x + y * width as usize] += inside as u16;
                                }
                            }
                        });
                    }
                });

                let super_count_buffer = super_count_buffer_cell
                    .into_iter()
                    .map(move |count_buffer_cell| count_buffer_cell.into_inner())
                    .collect::<Vec<_>>();

                surface
                    .resize(
                        NonZeroU32::new(width).unwrap(),
                        NonZeroU32::new(height).unwrap(),
                    )
                    .unwrap();

                let mut pixel_buffer = surface.buffer_mut().unwrap();
                pixel_buffer.iter_mut().for_each(|pixel| *pixel = 0);

                let pixel_chunk_len =
                    u32::max(width * height / self.threadpool.thread_count() / 10, 1) as usize;

                let pixel_buffer_chunks = pixel_buffer
                    .chunks_exact_mut(pixel_chunk_len)
                    .collect::<Vec<_>>();

                // let super_count_buffer = super_count_buffer_mutex
                //     .into_iter()
                //     .map(|count_buffer_mutex| count_buffer_mutex.into_inner().unwrap())
                //     .collect::<Vec<_>>();

                let super_count_buffer_chunks = super_count_buffer
                    .iter()
                    .map(|count_buffer| {
                        count_buffer
                            .chunks_exact(pixel_chunk_len)
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();

                self.threadpool.scoped(|scope| {
                    for (i_chunk, pixel_buffer_chunk) in pixel_buffer_chunks.into_iter().enumerate()
                    {
                        let mut count_chunks = Vec::new();
                        for count_buffer_chunks in &super_count_buffer_chunks {
                            count_chunks.push(count_buffer_chunks[i_chunk]);
                        }
                        scope.execute(move |_| {
                            for (i_pixel, pixel) in pixel_buffer_chunk.iter_mut().enumerate() {
                                let index = i_chunk * pixel_chunk_len + i_pixel;
                                let x = (index % width as usize) as f32;
                                let y = (index / width as usize) as f32;
                                let count = count_chunks.iter().fold(0_u16, |a, b| a + b[i_pixel])
                                    as f32
                                    * 32.0; // for pixel in pixel_buffer_chunk.iter_mut() {
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
                window.request_redraw();
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
