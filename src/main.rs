use minifb::{Key, MouseButton, Window, WindowOptions};
use particles::Particles;
use std::time::SystemTime;
mod particles;

const WIDTH: usize = 1280;
const HEIGHT: usize = 720;

fn main() {
    let mut buffer = [0; WIDTH * HEIGHT];
    let mut last_frame_time = SystemTime::now();
    let mut n_frame: u32 = 0;

    let mut particles = Particles::new(10_000_000, WIDTH, HEIGHT);

    let mut window = Window::new(
        "Test - ESC to exit",
        WIDTH,
        HEIGHT,
        WindowOptions::default(),
    )
    .unwrap();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        n_frame += 1;
        let now = SystemTime::now();
        let frametime = now.duration_since(last_frame_time).unwrap_or_default();
        last_frame_time = now;
        if n_frame % 100 == 0 {
            println!("#{}: FPS = {}", n_frame, 1.0 / frametime.as_secs_f32());
        }

        for i in buffer.iter_mut() {
            *i = 0; // write something more funny here!
        }

        particles.update(
            &frametime,
            window.get_unscaled_mouse_pos(minifb::MouseMode::Discard),
            window.get_mouse_down(MouseButton::Left),
        );

        for particle in &particles.particles {
            if particle.x < 0.0
                || particle.x >= WIDTH as f32
                || particle.y < 0.0
                || particle.y >= HEIGHT as f32
            {
                continue;
            }
            let x = particle.x as usize;
            let y = particle.y as usize;
            let red = particle.x * 255.0 / WIDTH as f32;
            let green = particle.y * 255.0 / HEIGHT as f32;
            let blue: f32 =
                (1.0 - (particle.x / WIDTH as f32) - (particle.y / HEIGHT as f32)) * 255.0;
            buffer[x + y * WIDTH] = ((red as u32) << 16) + ((green as u32) << 8) + (blue as u32);
        }

        // We unwrap here as we want this code to exit if it fails. Real applications may want to handle this in a different way
        window.update_with_buffer(&buffer, WIDTH, HEIGHT).unwrap();
    }
}
