//! Constraint preprocessing for the Sumzle solver

use crate::types::*;
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};

impl GlobalKnowledge {
    /// Build global knowledge from guess rows
    pub fn from_guess_rows(length: usize, rows: &[GuessRow]) -> Result<Self> {
        let mut gk = GlobalKnowledge {
            fixed_chars: vec![None; length],
            cannot_be_at: vec![HashSet::new(); length],
            must_appear_min_count: HashMap::new(),
            must_appear_exact_count: HashMap::new(),
            globally_forbidden: HashSet::new(),
        };

        // First pass: collect fixed chars and cannot-be-at constraints
        for row in rows {
            for (c, tile) in row.iter().enumerate() {
                if c >= length || tile.char == '\0' || tile.char == ' ' {
                    continue;
                }
                let ch = tile.char;
                match tile.state {
                    TileState::Correct => {
                        if let Some(fixed) = gk.fixed_chars[c] {
                            if fixed != ch {
                                return Err(anyhow!(
                                    "冲突: 位置 {} 同时固定为 {} 和 {}.",
                                    c + 1,
                                    fixed,
                                    ch
                                ));
                            }
                        }
                        gk.fixed_chars[c] = Some(ch);
                        // All other chars cannot be at this position
                        for &vc in VALID_CHARS.iter() {
                            let vc = vc as char;
                            if vc != ch {
                                gk.cannot_be_at[c].insert(vc);
                            }
                        }
                    }
                    TileState::Present => {
                        gk.cannot_be_at[c].insert(ch);
                    }
                    TileState::Empty => {
                        gk.cannot_be_at[c].insert(ch);
                    }
                }
            }
        }

        // Collect all characters that appear in any guess
        let mut all_chars_in_guesses: HashSet<char> = HashSet::new();
        for row in rows {
            for tile in row {
                if tile.char != '\0' && tile.char != ' ' {
                    all_chars_in_guesses.insert(tile.char);
                }
            }
        }

        // Second pass: determine min/exact counts
        for &ch in &all_chars_in_guesses {
            let mut min_required_overall: usize = 0;
            let mut derived_exact_count: Option<usize> = None;

            for row in rows {
                if !row.iter().any(|t| t.char == ch) {
                    continue;
                }

                let mut green_in_row: usize = 0;
                let mut yellow_in_row: usize = 0;
                for tile in row {
                    if tile.char == ch {
                        match tile.state {
                            TileState::Correct => green_in_row += 1,
                            TileState::Present => yellow_in_row += 1,
                            TileState::Empty => {}
                        }
                    }
                }

                let min_required_this_row = green_in_row + yellow_in_row;
                min_required_overall = min_required_overall.max(min_required_this_row);

                // If any tile with this char has state Empty, we know the exact count
                if row.iter().any(|t| t.char == ch && t.state == TileState::Empty) {
                    let exact_this_row = green_in_row + yellow_in_row;
                    match derived_exact_count {
                        None => derived_exact_count = Some(exact_this_row),
                        Some(prev) => {
                            if prev != exact_this_row {
                                return Err(anyhow!(
                                    "冲突: 字符 '{}' 在不同猜测行中推断出不同的精确数量 ({} vs {}).",
                                    ch, prev, exact_this_row
                                ));
                            }
                        }
                    }
                }
            }

            gk.must_appear_min_count.insert(ch, min_required_overall);

            if let Some(exact) = derived_exact_count {
                if exact < min_required_overall {
                    return Err(anyhow!(
                        "冲突: 字符 '{}' 的精确数量 ({}) 小于其最小需求数量 ({}).",
                        ch, exact, min_required_overall
                    ));
                }
                gk.must_appear_exact_count.insert(ch, exact);
                if exact == 0 && min_required_overall == 0 {
                    gk.globally_forbidden.insert(ch);
                }
            }
        }

        // Validate fixed chars don't conflict
        for i in 0..length {
            if let Some(fixed) = gk.fixed_chars[i] {
                if gk.globally_forbidden.contains(&fixed) {
                    return Err(anyhow!(
                        "冲突: 字符 '{}' 在位置 {} 固定但同时被全局禁用.",
                        fixed, i + 1
                    ));
                }
                if gk.cannot_be_at[i].contains(&fixed) {
                    return Err(anyhow!(
                        "冲突: 字符 '{}' 在位置 {} 固定但又标记为不能在该位置.",
                        fixed, i + 1
                    ));
                }
                let min_count = gk.must_appear_min_count.get(&fixed).copied().unwrap_or(0);
                gk.must_appear_min_count
                    .insert(fixed, min_count.max(1));
                if let Some(exact) = gk.must_appear_exact_count.get(&fixed) {
                    let min = gk.must_appear_min_count.get(&fixed).copied().unwrap_or(0);
                    if *exact < min {
                        return Err(anyhow!(
                            "冲突: 字符 '{}' 的精确数量 {} 小于其最小固定要求 {}.",
                            fixed, exact, min
                        ));
                    }
                }
            }
        }

        // Validate exact counts >= min counts
        for (&ch, &exact) in &gk.must_appear_exact_count {
            let min = gk.must_appear_min_count.get(&ch).copied().unwrap_or(0);
            if exact < min {
                return Err(anyhow!(
                    "冲突: 字符 '{}' 的精确数量 ({}) 小于其最小需求 ({}).",
                    ch, exact, min
                ));
            }
        }

        // Validate globally forbidden chars don't conflict
        for &ch in &gk.globally_forbidden {
            let min = gk.must_appear_min_count.get(&ch).copied().unwrap_or(0);
            if min > 0 {
                return Err(anyhow!(
                    "冲突: 字符 '{}' 被全局禁用但又要求至少出现.",
                    ch
                ));
            }
            if let Some(&exact) = gk.must_appear_exact_count.get(&ch) {
                if exact > 0 {
                    return Err(anyhow!(
                        "冲突: 字符 '{}' 被全局禁用但又要求精确出现.",
                        ch
                    ));
                }
            }
        }

        Ok(gk)
    }
}
