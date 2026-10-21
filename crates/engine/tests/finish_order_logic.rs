//! impl-1132: the ordering logic of the vendored `bevy_render` parallel encoder finish, tested
//! without a GPU device. `finish_in_order` is the same function `PendingCommandBuffers::take` runs
//! (there with `CommandBuffer` and `CommandEncoder`); here the buffers and encoders are plain
//! numbers, so the order of the result is checked on any machine, including one with no adapter.
//! `parallel_finish_order.rs` checks the same thing on a real device.

use bevy::render::renderer::{Pending, finish_in_order, last_parallel_finish_count};
use bevy::tasks::{ComputeTaskPool, TaskPool};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

/// The tests share the pool and the "last finish" counter, so they run one at a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A finished buffer `n`, or an unfinished encoder `n` that finishes into `n + 1000`.
fn entry(n: u32, encoder: bool) -> Pending<u32, u32> {
    if encoder {
        Pending::Encoder(n)
    } else {
        Pending::Buffer(n)
    }
}

fn expected(entries: &[(u32, bool)]) -> Vec<u32> {
    entries
        .iter()
        .map(|&(n, encoder)| if encoder { n + 1000 } else { n })
        .collect()
}

fn run(entries: &[(u32, bool)], use_pool: bool) -> Vec<u32> {
    let pending = entries
        .iter()
        .map(|&(n, encoder)| entry(n, encoder))
        .collect();
    finish_in_order(pending, use_pool, |encoder| encoder + 1000)
}

#[test]
fn a_mixed_sequence_comes_back_in_its_own_order_serial_and_parallel() {
    let _one = lock();
    ComputeTaskPool::get_or_init(TaskPool::default);
    // Buffers (add_command_buffer), encoders (deferred flushes), and a trailing encoder (the
    // diagnostics `push_encoder`), in an order a sorted or grouped result would get wrong.
    let entries = [
        (1, false),
        (2, true),
        (3, false),
        (4, true),
        (5, true),
        (6, false),
        (7, true),
    ];
    assert_eq!(run(&entries, false), expected(&entries), "switch off");
    assert_eq!(
        last_parallel_finish_count(),
        0,
        "off: nothing went through the pool"
    );
    assert_eq!(run(&entries, true), expected(&entries), "switch on");
    assert_eq!(
        last_parallel_finish_count(),
        4,
        "on: four encoders went through the pool"
    );
}

#[test]
fn many_encoders_finishing_out_of_order_still_return_in_order() {
    let _one = lock();
    ComputeTaskPool::get_or_init(TaskPool::default);
    // Every third entry is a finished buffer; the encoders take different times to finish, so
    // completion order is scrambled and only the placement can keep the result in order.
    let entries: Vec<(u32, bool)> = (0..300).map(|n| (n, n % 3 != 0)).collect();
    let started = AtomicUsize::new(0);
    let pending = entries
        .iter()
        .map(|&(n, encoder)| entry(n, encoder))
        .collect();
    let result = finish_in_order(pending, true, |encoder| {
        started.fetch_add(1, Ordering::Relaxed);
        std::thread::sleep(Duration::from_micros(u64::from((300 - encoder) % 7) * 40));
        encoder + 1000
    });
    assert_eq!(result, expected(&entries));
    let encoders = entries.iter().filter(|entry| entry.1).count();
    assert_eq!(
        started.load(Ordering::Relaxed),
        encoders,
        "each encoder finished exactly once"
    );
    assert_eq!(last_parallel_finish_count(), encoders);
}

#[test]
fn fewer_than_two_encoders_finish_serially_even_with_the_switch_on() {
    let _one = lock();
    ComputeTaskPool::get_or_init(TaskPool::default);
    let one = [(1, false), (2, true), (3, false)];
    assert_eq!(run(&one, true), expected(&one));
    assert_eq!(last_parallel_finish_count(), 0);
    let none = [(1, false), (2, false)];
    assert_eq!(run(&none, true), expected(&none));
    assert_eq!(last_parallel_finish_count(), 0);
    assert_eq!(run(&[], true), Vec::<u32>::new());
}

#[test]
fn an_encoder_only_list_and_a_buffer_only_list_keep_their_order() {
    let _one = lock();
    ComputeTaskPool::get_or_init(TaskPool::default);
    let encoders: Vec<(u32, bool)> = (0..10).map(|n| (n, true)).collect();
    assert_eq!(run(&encoders, true), expected(&encoders));
    assert_eq!(last_parallel_finish_count(), 10);
    let buffers: Vec<(u32, bool)> = (0..10).map(|n| (n, false)).collect();
    assert_eq!(run(&buffers, true), expected(&buffers));
    assert_eq!(last_parallel_finish_count(), 0);
}
