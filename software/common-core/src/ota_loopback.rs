//! Sender and receiver wired together through a bus that loses and damages
//! frames: whatever it does to them, the image either arrives intact or the
//! update fails cleanly — never a wrong image reported as good.

extern crate std;

use crate::ota::{
    crc16, frames, OtaControl, BEGIN_TIMEOUT_MS, BLOCK_SIZE, BLOCK_TIMEOUT_MS, END_TIMEOUT_MS,
    MAX_RETRIES, STALL_MS,
};
use crate::ota_receiver::{Effect, OtaReceiver};
use crate::ota_sender::{BlockSender, Failure, Verdict};
use std::collections::VecDeque;
use std::vec::Vec;

/// xorshift64*: enough randomness for a fault model, and reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn chance(&mut self, probability: f64) -> bool {
        ((self.next() >> 11) as f64 / (1u64 << 53) as f64) < probability
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[derive(Debug, Default)]
struct Stats {
    frames_sent: u32,
}

/// Everything but the receiver itself.
struct World {
    rng: Rng,
    /// Chance that a frame is lost on its way, in either direction.
    loss: f64,
    /// Chance that a data frame arrives with a damaged bit nobody noticed.
    corrupt: f64,
    now: u64,
    flash: Vec<u8>,
    expected_crc32: u32,
    /// What the node has sent and the bus has not lost.
    replies: VecDeque<OtaControl>,
    stats: Stats,
    verified_ok: bool,
}

/// Carries out what the receiver asks for until it has nothing more to do.
fn run(
    receiver: &mut OtaReceiver,
    world: &mut World,
    lossy_replies: bool,
    first: impl for<'a> FnOnce(&'a mut OtaReceiver) -> Effect<'a>,
) {
    let mut effect = first(receiver);
    loop {
        effect = match effect {
            Effect::None => return,
            Effect::Reply(reply) => {
                if !(lossy_replies && world.rng.chance(world.loss)) {
                    world.replies.push_back(reply);
                }
                return;
            }
            Effect::Begin { .. } => receiver.begun(true),
            Effect::WriteSector { offset, data } => {
                let offset = offset as usize;
                world.flash[offset..offset + data.len()].copy_from_slice(data);
                receiver.sector_written(true)
            }
            Effect::Verify => {
                world.verified_ok = crc32(&world.flash) == world.expected_crc32;
                receiver.verified(world.verified_ok)
            }
        };
    }
}

struct Sim {
    receiver: OtaReceiver,
    world: World,
}

impl Sim {
    fn new(seed: u64, loss: f64, corrupt: f64, image: &[u8]) -> Self {
        Self {
            receiver: OtaReceiver::new(),
            world: World {
                rng: Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
                loss,
                corrupt,
                now: 0,
                flash: std::vec![0xFF; image.len()],
                expected_crc32: crc32(image),
                replies: VecDeque::new(),
                stats: Stats::default(),
                verified_ok: false,
            },
        }
    }

    fn control(&mut self, frame: OtaControl, lossy: bool) {
        self.world.stats.frames_sent += 1;
        if lossy && self.world.rng.chance(self.world.loss) {
            return;
        }
        let now = self.world.now;
        run(&mut self.receiver, &mut self.world, lossy, |r| {
            r.control(frame, now)
        });
    }

    fn data(&mut self, mut bytes: [u8; 8]) {
        self.world.stats.frames_sent += 1;
        if self.world.rng.chance(self.world.loss) {
            return;
        }
        if self.world.rng.chance(self.world.corrupt) {
            let i = (self.world.rng.next() % 8) as usize;
            bytes[i] ^= 1 << (self.world.rng.next() % 8);
        }
        let now = self.world.now;
        run(&mut self.receiver, &mut self.world, true, |r| {
            r.data(&bytes, now)
        });
    }

    /// The sender waits up to `timeout_ms` for an answer.
    fn wait_reply(&mut self, timeout_ms: u32) -> Option<OtaControl> {
        // The node notices a stalled block 300 ms after its last frame.
        self.world.now += u64::from(STALL_MS);
        let now = self.world.now;
        run(&mut self.receiver, &mut self.world, true, |r| r.tick(now));
        match self.world.replies.pop_front() {
            Some(reply) => Some(reply),
            None => {
                self.world.now += u64::from(timeout_ms);
                None
            }
        }
    }

    /// The whole update the way the gateway does it. `Ok` carries the number
    /// of blocks that had to be sent again.
    fn update(&mut self, image: &[u8], block_size: usize) -> Result<u32, Failure> {
        // Begin: repeated if it is not answered.
        let begin = OtaControl::Begin {
            block_size: block_size as u16,
            size: image.len() as u32,
            crc32: crc32(image),
        };
        let mut begun = false;
        for _ in 0..=MAX_RETRIES {
            self.control(begin, true);
            match self.wait_reply(BEGIN_TIMEOUT_MS) {
                Some(OtaControl::Ack { next: 0 }) => {
                    begun = true;
                    break;
                }
                Some(OtaControl::Error(code)) => return Err(Failure::Node(code)),
                _ => {}
            }
        }
        if !begun {
            return Err(Failure::TooManyRetries);
        }

        let mut retries = 0u32;
        for (index, block) in image.chunks(block_size).enumerate() {
            let mut sender = BlockSender::new(index as u16, block);
            'send: loop {
                self.control(sender.header(), true);
                for frame in frames(block) {
                    self.data(frame);
                }
                loop {
                    let reply = self.wait_reply(BLOCK_TIMEOUT_MS);
                    match sender.answer(reply) {
                        Verdict::Acked => break 'send,
                        Verdict::Retry => {
                            retries += 1;
                            continue 'send;
                        }
                        Verdict::Wait => continue,
                        Verdict::Failed(why) => return Err(why),
                    }
                }
            }
        }

        // End and the answer are not lossy here: the real node restarts
        // after `Done`, so a lost one is not recoverable by asking again.
        self.control(OtaControl::End, false);
        match self.wait_reply(END_TIMEOUT_MS) {
            Some(OtaControl::Done) => Ok(retries),
            Some(OtaControl::Error(code)) => Err(Failure::Node(code)),
            _ => Err(Failure::TooManyRetries),
        }
    }
}

fn image(len: usize, seed: u8) -> Vec<u8> {
    let mut rng = Rng(u64::from(seed) * 7919 + 13);
    (0..len).map(|_| rng.next() as u8).collect()
}

#[test]
fn a_clean_bus_needs_no_retries() {
    for len in [1usize, 7, 1024, 1025, 4096, 4097, 100_003] {
        let image = image(len, 1);
        let mut sim = Sim::new(1, 0.0, 0.0, &image);
        assert_eq!(sim.update(&image, BLOCK_SIZE), Ok(0), "{len}");
        assert_eq!(sim.world.flash, image, "{len}");
        assert!(sim.world.verified_ok);
        // One header and 1/8 frame per byte per block, plus Begin and End.
        let blocks = len.div_ceil(BLOCK_SIZE) as u32;
        let data_frames: u32 = image
            .chunks(BLOCK_SIZE)
            .map(|b| b.len().div_ceil(8) as u32)
            .sum();
        assert_eq!(sim.world.stats.frames_sent, 2 + blocks + data_frames);
    }
}

/// Runs updates of different images over a bus with this fault model and
/// returns how many blocks were sent again and how many updates failed.
fn campaign(loss: f64, corrupt: f64, block_size: usize, seeds: u64) -> (u32, u32) {
    let mut repeated = 0u32;
    let mut failed = 0u32;
    for seed in 1..=seeds {
        let image = image(30_000 + seed as usize * 13, seed as u8);
        let mut sim = Sim::new(seed, loss, corrupt, &image);
        match sim.update(&image, block_size) {
            Ok(retries) => {
                assert_eq!(sim.world.flash, image, "loss {loss} seed {seed}");
                assert!(sim.world.verified_ok);
                repeated += retries;
            }
            // Too many unlucky tries on one block: the update stops. It
            // never carries on with a wrong image.
            Err(Failure::TooManyRetries) => {
                assert!(!sim.world.verified_ok, "loss {loss} seed {seed}");
                failed += 1;
            }
            Err(other) => panic!("loss {loss} seed {seed}: {other:?}"),
        }
    }
    (repeated, failed)
}

#[test]
fn the_image_arrives_intact_over_a_bus_that_loses_frames() {
    // 1 frame in 500 lost, 1 in 3000 damaged: a block of 128 frames is
    // repeated now and then, and the update always gets through.
    let (repeated, failed) = campaign(0.002, 0.0003, BLOCK_SIZE, 30);
    assert!(
        repeated > 10,
        "only {repeated} repeated blocks: no fault bites"
    );
    assert_eq!(failed, 0);
}

#[test]
fn smaller_blocks_ride_out_a_worse_bus() {
    // 1 frame in 100: whole 1 KiB blocks (128 frames) mostly fail, 256-byte
    // blocks (32 frames) get through. This is what the block size is for.
    let (_, failed_large) = campaign(0.01, 0.001, BLOCK_SIZE, 10);
    let (repeated, failed_small) = campaign(0.01, 0.001, 256, 10);
    assert!(failed_large > failed_small);
    assert!(repeated > 100);
    assert_eq!(failed_small, 0);
}

#[test]
fn a_realistic_loss_rate_always_gets_through() {
    // About one frame in a thousand is lost or damaged.
    for seed in 1..=40u64 {
        let image = image(60_000, seed as u8);
        let mut sim = Sim::new(seed, 0.0007, 0.0003, &image);
        let retries = sim.update(&image, BLOCK_SIZE).expect("update failed");
        assert_eq!(sim.world.flash, image);
        assert!(retries <= 30, "{retries}");
    }
}

#[test]
fn a_bus_that_is_mostly_down_fails_instead_of_hanging() {
    let image = image(20_000, 3);
    let mut sim = Sim::new(3, 0.6, 0.0, &image);
    assert!(sim.update(&image, BLOCK_SIZE).is_err());
    assert!(!sim.world.verified_ok);
}

#[test]
fn a_damaged_frame_the_crc_catches_is_not_written() {
    // Corrupt every data frame: nothing passes the CRC, nothing is flashed.
    let image = image(5_000, 9);
    let mut sim = Sim::new(9, 0.0, 1.0, &image);
    assert_eq!(sim.update(&image, BLOCK_SIZE), Err(Failure::TooManyRetries));
    assert!(sim.world.flash.iter().all(|b| *b == 0xFF));
}

#[test]
fn the_block_crc_is_what_the_header_promises() {
    let block = image(BLOCK_SIZE, 5);
    let sender = BlockSender::new(0, &block);
    assert_eq!(
        sender.header(),
        OtaControl::Block {
            index: 0,
            len: BLOCK_SIZE as u16,
            crc16: crc16(&block)
        }
    );
}
