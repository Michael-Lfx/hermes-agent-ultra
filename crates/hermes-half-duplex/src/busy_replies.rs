//! Short spoken phrases when the agent is still processing a prior turn.

use std::time::{Duration, Instant};

use rand::seq::IndexedRandom;

const BUSY_REPLIES: &[&str] = &[
    "稍等一下，我马上回复你。",
    "上一条我还没说完，等我一下。",
    "让我先把手头这件事做完。",
    "听到了，稍等几秒钟。",
    "正在想，马上就好。",
    "别急，我还在处理。",
    "等我一下，就快了。",
    "正在生成回复，请稍候。",
    "工具还在运行，稍等一下。",
    "我还在查资料，马上。",
    "收到，让我想想怎么说。",
    "你刚说的我记下了，等我忙完这条。",
    "还在跑模型，稍候。",
    "先别急着说下一句，这条还没完。",
    "我这边还在忙，马上听你的。",
    "等一下，我回复完这条就听你的。",
    "处理中，请稍等。",
    "好的好的，稍等片刻。",
    "让我组织一下语言，马上。",
    "还在思考，几秒钟就好。",
    "上一条回复还没结束，稍等。",
    "我听到了，处理完当前的就回应你。",
    "正在为你准备回答，请稍候。",
    "稍等，我把工具结果看完。",
    "马上，还在工作中。",
    "等等我，这条快好了。",
    "正在处理你的上一个问题。",
];

/// Pick a random busy phrase, avoiding `last` when possible.
pub fn pick_busy_reply(last: Option<&str>) -> &'static str {
    let mut rng = rand::rng();
    if BUSY_REPLIES.len() <= 1 {
        return BUSY_REPLIES[0];
    }
    for _ in 0..8 {
        if let Some(choice) = BUSY_REPLIES.choose(&mut rng) {
            if last != Some(*choice) {
                return choice;
            }
        }
    }
    BUSY_REPLIES[0]
}

pub struct BusyReplyGate {
    cooldown: Duration,
    last_played: Option<Instant>,
    last_phrase: Option<String>,
}

impl BusyReplyGate {
    pub fn new(cooldown_secs: u64) -> Self {
        Self {
            cooldown: Duration::from_secs(cooldown_secs.max(1)),
            last_played: None,
            last_phrase: None,
        }
    }

    pub fn should_play(&self) -> bool {
        match self.last_played {
            None => true,
            Some(t) => t.elapsed() >= self.cooldown,
        }
    }

    pub fn pick(&mut self) -> &'static str {
        let phrase = pick_busy_reply(self.last_phrase.as_deref());
        self.last_phrase = Some(phrase.to_string());
        phrase
    }

    pub fn mark_played(&mut self) {
        self.last_played = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_is_non_empty() {
        assert!(!pick_busy_reply(None).is_empty());
    }

    #[test]
    fn gate_respects_cooldown() {
        let mut gate = BusyReplyGate::new(10);
        assert!(gate.should_play());
        gate.mark_played();
        assert!(!gate.should_play());
    }
}
