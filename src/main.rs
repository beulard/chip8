extern crate sdl3;

mod font;

use core::panic;
use sdl3::keyboard::Keycode;
use sdl3::pixels::Color;
use sdl3::rect::Point;
use sdl3::{event::Event, render::WindowCanvas};
use std::time::{Duration, Instant};

/// Display scale factor.
const SCALE_FACTOR: u32 = 16;

/// Target frame time.
const FRAMETIME_US: u128 = 16667;

const TIMER_DECREMENT_INTERVAL_US: u128 = 16667;

/// Number of microseconds between two chip8 clock cycles.
const CHIP8_UPDATE_TIME_US: u128 = 1429; // 1429 = 1000000 / 700 (700Hz)

/// LIFO stack
const STACK_CAPACITY: usize = 64;

#[derive(Debug)]
#[allow(unused)]
struct Chip8Stack {
    buffer: [u16; STACK_CAPACITY],
    top: usize,
}

#[allow(unused)]
impl Chip8Stack {
    fn new() -> Self {
        return Chip8Stack {
            buffer: [0; _],
            top: 0,
        };
    }
    fn push(&mut self, value: u16) {
        if self.top == STACK_CAPACITY {
            panic!();
        }
        self.buffer[self.top] = value;
        self.top += 1;
    }
    fn pop(&mut self) -> u16 {
        if self.top == 0 {
            panic!();
        }
        self.top -= 1;
        return self.buffer[self.top];
    }
}

#[allow(unused)]
#[derive(Debug)]
struct Chip8State {
    ram: [u8; 4096],
    /// Program counter.
    pc: u16,
    /// Index register.
    i: u16,
    /// General purpose registers.
    v0: u8,
    v1: u8,
    v2: u8,
    v3: u8,
    v4: u8,
    v5: u8,
    v6: u8,
    v7: u8,
    v8: u8,
    v9: u8,
    va: u8,
    vb: u8,
    vc: u8,
    vd: u8,
    ve: u8,
    vf: u8,
    delay_timer: u8,
    sound_timer: u8,
    stack: Chip8Stack,

    //
    elapsed_us: u128,
}

impl Chip8State {
    fn new() -> Self {
        let mut ram: [u8; _] = [0; 4096];

        // Copy font into ram
        ram[0x50..=0x9F].copy_from_slice(&font::FONT);

        Chip8State {
            ram: ram,
            pc: 0,
            i: 0,
            v0: 0,
            v1: 0,
            v2: 0,
            v3: 0,
            v4: 0,
            v5: 0,
            v6: 0,
            v7: 0,
            v8: 0,
            v9: 0,
            va: 0,
            vb: 0,
            vc: 0,
            vd: 0,
            ve: 0,
            vf: 0,
            delay_timer: 0,
            sound_timer: 0,
            stack: Chip8Stack::new(),
            elapsed_us: 0,
        }
    }
    fn update(&mut self, delta: Duration) {
        // Update timers
        self.elapsed_us += delta.as_micros();
        while self.elapsed_us >= TIMER_DECREMENT_INTERVAL_US {
            // println!("decrement timers");
            if self.delay_timer > 0 {
                self.delay_timer -= 1;
            }
            if self.sound_timer > 0 {
                self.sound_timer -= 1;
            }
            self.elapsed_us -= TIMER_DECREMENT_INTERVAL_US;
        }

        // Fetch
        let instr = self.ram[self.pc..=self.pc + 1];
    }
}

pub fn main() {
    let sdl_context = sdl3::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();

    let window = video_subsystem
        .window("chip8 interpreter", 64 * SCALE_FACTOR, 32 * SCALE_FACTOR)
        .position_centered()
        .borderless()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas();

    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.present();

    let mut event_pump = sdl_context.event_pump().unwrap();

    let mut prev_update = Instant::now();
    let mut lag_us = 0;
    let mut prev_render = Instant::now();

    let mut chip8_state = Chip8State::new();

    'running: loop {
        // Handle events
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }

        // Update in as many fixed steps
        lag_us += prev_update.elapsed().as_micros();
        // Number of cycles to simulate.
        while lag_us >= CHIP8_UPDATE_TIME_US {
            // println!("lag_us={} us_per_update={}", lag_us, CHIP8_UPDATE_TIME_US);
            let delta = prev_update.elapsed();
            prev_update = Instant::now();

            // let update_start = Instant::now();
            chip8_state.update(delta);

            // println!("update time: {} us", update_start.elapsed().as_micros());
            lag_us -= CHIP8_UPDATE_TIME_US;
        }

        if prev_render.elapsed().as_micros() > FRAMETIME_US {
            let framerate = 1.0 / prev_render.elapsed().as_secs_f64();
            prev_render = Instant::now();
            render(&mut canvas, framerate);
            // let render_time = prev_render.elapsed();
            // println!("render time: {} us", render_time.as_micros());
        }
    }
}

fn render(canvas: &mut WindowCanvas, framerate: f64) {
    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.set_draw_color(Color::RGB(165, 165, 165));
    canvas
        .draw_debug_text(&format!("{:.1}", framerate), Point::new(5, 5))
        .unwrap();
    canvas.present();
}
