use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(super) const VERSION: &str = "sha256-d6-v1";
pub(super) const TURN_LIMIT: usize = 1024;
const OPENING_LIMIT: usize = 64;

#[derive(Clone, Copy)]
pub(super) enum RollKind {
    Opening = 0,
    Turn = 1,
}

pub(super) struct Dice {
    seed: Uuid,
    game: i32,
    kind: RollKind,
    block: u32,
    bytes: [u8; 32],
    offset: usize,
}

impl Dice {
    pub(super) fn new(seed: Uuid, game: i32, kind: RollKind) -> Self {
        Self {
            seed,
            game,
            kind,
            block: 0,
            bytes: [0; 32],
            offset: 32,
        }
    }

    fn die(&mut self) -> u8 {
        loop {
            if self.offset == self.bytes.len() {
                let mut hash = Sha256::new();
                hash.update(VERSION);
                hash.update(self.seed.as_bytes());
                hash.update(self.game.to_be_bytes());
                hash.update([self.kind as u8]);
                hash.update(self.block.to_be_bytes());
                self.bytes = hash.finalize().into();
                self.block += 1;
                self.offset = 0;
            }
            let byte = self.bytes[self.offset];
            self.offset += 1;
            if byte < 252 {
                return 1 + byte % 6;
            }
        }
    }

    pub(super) fn roll(&mut self) -> [u8; 2] {
        [self.die(), self.die()]
    }
}

pub(super) fn opening(seed: Uuid, game: i32) -> anyhow::Result<Vec<[u8; 2]>> {
    let mut dice = Dice::new(seed, game, RollKind::Opening);
    let mut rolls = Vec::new();
    for _ in 0..OPENING_LIMIT {
        let roll = dice.roll();
        rolls.push(roll);
        if roll[0] != roll[1] {
            return Ok(rolls);
        }
    }
    anyhow::bail!("Opening roll tie limit exceeded")
}

pub(super) fn turns(seed: Uuid, game: i32) -> Vec<[u8; 2]> {
    let mut dice = Dice::new(seed, game, RollKind::Turn);
    (0..TURN_LIMIT).map(|_| dice.roll()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_are_versioned_and_independent_by_game_and_roll_type() {
        let seed = Uuid::nil();
        let mut opening = Dice::new(seed, 1, RollKind::Opening);
        let mut turns = Dice::new(seed, 1, RollKind::Turn);
        let sequence: Vec<_> = (0..64).map(|_| turns.roll()).collect();
        for roll in &sequence {
            assert!(roll.iter().all(|die| (1..=6).contains(die)));
        }
        assert_eq!(opening.roll(), [1, 5]);
        assert_eq!(&sequence[..4], &[[3, 2], [3, 1], [1, 6], [6, 3]]);
        assert_eq!(Dice::new(seed, 1, RollKind::Turn).roll(), sequence[0]);
        assert_ne!(Dice::new(seed, 2, RollKind::Turn).roll(), sequence[0]);
    }
}
