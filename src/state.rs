use crate::tehai::Tehai;
use crate::{log, log_if};

use anyhow::anyhow;
use anyhow::{Context, Result};
use convlog::mjai::{Consumed2, Consumed3, Consumed4, Event};
use convlog::Pai;
use itertools::{EitherOrBoth::*, Itertools};
use serde::Serialize;
use serde_with::{serde_as, DisplayFromStr};
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::fmt;

#[derive(Debug, Clone, Default, Serialize)]
pub struct State {
    #[serde(skip)]
    actor: u8,

    pub tehai: Tehai,
    pub fuuros: Vec<Fuuro>,
}

struct PaiIterator<'a> {
    tehai: std::slice::Iter<'a, convlog::Pai>,
    fuuros: std::slice::Iter<'a, Fuuro>,
    cur_fuuro: Option<Vec<Pai>>,
    cur_fuuro_index: usize,
}

impl<'a> Iterator for PaiIterator<'a> {
    type Item = Pai;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(pai) = self.tehai.next() {
            return Some(*pai);
        }

        // get pai from current fuuro
        if let Some(curr) = &self.cur_fuuro {
            if self.cur_fuuro_index < curr.len() {
                let pai = curr[self.cur_fuuro_index];
                self.cur_fuuro_index += 1;
                return Some(pai);
            }
        }
        // get pai from next fuuro
        if let Some(next_fuuro) = self.fuuros.next() {
            let pais = next_fuuro.into_pais();
            let first_pai = pais[0];
            self.cur_fuuro = Some(pais);
            self.cur_fuuro_index = 1;
            return Some(first_pai);
        }
        // nothing left
        None
    }
}

impl State {
    #[inline]
    pub fn new(actor: u8) -> Self {
        State {
            actor,
            ..Self::default()
        }
    }

    /// Argument `event` must be one of
    ///
    /// * StartKyoku
    /// * Tsumo
    /// * Dahai
    /// * Chi
    /// * Pon
    /// * Kakan
    /// * Daiminkan
    /// * Ankan
    ///
    /// and the `actor` must be the target actor.
    ///
    /// Otherwise this is a no-op.
    pub fn update(&mut self, event: &Event) -> Result<()> {
        match *event {
            Event::StartKyoku { tehais, .. } => {
                self.tehai.haipai(&tehais[self.actor as usize]);
                self.fuuros.clear();
            }

            Event::Tsumo { actor, pai } if actor == self.actor => self.tehai.tsumo(pai),

            Event::Dahai {
                actor,
                pai,
                tsumogiri,
            } if actor == self.actor => {
                if tsumogiri {
                    self.tehai.tsumogiri();
                } else {
                    self.tehai.tedashi(pai);
                }
            }

            Event::Chi {
                actor,
                target,
                pai,
                consumed,
            } if actor == self.actor => {
                self.tehai.remove_multiple(&consumed.as_array());

                let fuuro = Fuuro::Chi {
                    target,
                    pai,
                    consumed,
                };
                self.fuuros.push(fuuro);
            }

            Event::Pon {
                actor,
                target,
                pai,
                consumed,
            } if actor == self.actor => {
                self.tehai.remove_multiple(&consumed.as_array());

                let fuuro = Fuuro::Pon {
                    target,
                    pai,
                    consumed,
                };
                self.fuuros.push(fuuro);
            }

            Event::Daiminkan {
                actor,
                target,
                pai,
                consumed,
            } if actor == self.actor => {
                self.tehai.remove_multiple(&consumed.as_array());

                let fuuro = Fuuro::Daiminkan {
                    target,
                    pai,
                    consumed,
                };
                self.fuuros.push(fuuro);
            }

            Event::Kakan {
                actor,
                pai,
                consumed,
            } if actor == self.actor => {
                self.tehai.tedashi(pai);

                let (
                    previous_pon_idx,
                    previous_pon_target,
                    previous_pon_pai,
                    previous_pon_consumed,
                ) = self
                    .fuuros
                    .iter()
                    .enumerate()
                    .find_map(|(idx, f)| match *f {
                        Fuuro::Pon {
                            target: pon_target,
                            pai: pon_pai,
                            consumed: pon_consumed,
                        } if Consumed3::from([
                            pon_pai,
                            pon_consumed.as_array()[0],
                            pon_consumed.as_array()[1],
                        ]) == consumed =>
                        {
                            Some((idx, pon_target, pon_pai, pon_consumed))
                        }

                        _ => None,
                    })
                    .context(anyhow!("invalid state: previous Pon not found for Kakan"))?;

                let fuuro = Fuuro::Kakan {
                    pai,
                    previous_pon_target,
                    previous_pon_pai,
                    consumed: previous_pon_consumed,
                };
                self.fuuros[previous_pon_idx] = fuuro;
            }

            Event::Ankan { actor, consumed } if actor == self.actor => {
                self.tehai.remove_multiple(&consumed.as_array());

                let fuuro = Fuuro::Ankan { consumed };
                self.fuuros.push(fuuro);
            }

            _ => (),
        };

        Ok(())
    }

    fn iter(&self) -> PaiIterator {
        PaiIterator {
            tehai: self.tehai.view().iter(),
            fuuros: self.fuuros.iter(),
            cur_fuuro: None,
            cur_fuuro_index: 0,
        }
    }

    // calculate the shanten
    pub fn calc_shanten(&self) -> i32 {
        let mut s = ShantenHelper::new(&self.tehai);
        s.get_shanten()
    }
}

#[serde_as]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum Fuuro {
    Chi {
        target: u8,
        #[serde_as(as = "DisplayFromStr")]
        pai: Pai,
        consumed: Consumed2,
    },
    Pon {
        target: u8,
        #[serde_as(as = "DisplayFromStr")]
        pai: Pai,
        consumed: Consumed2,
    },
    Daiminkan {
        target: u8,
        #[serde_as(as = "DisplayFromStr")]
        pai: Pai,
        consumed: Consumed3,
    },
    Kakan {
        #[serde_as(as = "DisplayFromStr")]
        pai: Pai,
        previous_pon_target: u8,
        #[serde_as(as = "DisplayFromStr")]
        previous_pon_pai: Pai,
        consumed: Consumed2,
    },
    Ankan {
        consumed: Consumed4,
    },
}

impl Fuuro {
    fn into_pais(&self) -> Vec<Pai> {
        let mut return_pais: Vec<Pai> = Vec::new();
        match self {
            Self::Chi { pai, consumed, .. } => {
                return_pais.push(*pai);
                return_pais.extend_from_slice(&consumed.as_array());
            }
            Self::Pon { pai, consumed, .. } => {
                return_pais.push(*pai);
                return_pais.extend_from_slice(&consumed.as_array());
            }
            Self::Daiminkan { pai, consumed, .. } => {
                return_pais.push(*pai);
                return_pais.extend_from_slice(&consumed.as_array());
            }
            Self::Kakan {
                pai,
                previous_pon_pai,
                consumed,
                ..
            } => {
                return_pais.push(*pai);
                return_pais.push(*previous_pon_pai);
                return_pais.extend_from_slice(&consumed.as_array());
            }
            Self::Ankan { consumed } => {
                return_pais.extend_from_slice(&consumed.as_array());
            }
        }
        return_pais
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Distance {
    One,
    Two,
    Inf,
}
impl fmt::Display for Distance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                &Distance::Inf => '∞',
                &Distance::One => '1',
                &Distance::Two => '2',
            }
        )
    }
}

#[derive(Debug)]
struct BlockElem {
    pai: Pai,
    num: u32,
    // The distance between current Pai and next Pai
    distance: Distance,
}
type Block = Vec<BlockElem>;

const PAIS_VEC_LEN: usize = 48;

struct ShantenHelper {
    pais: [i32; PAIS_VEC_LEN],
    num_pais_rem: i32,
    num_tehai: i32,

    verbose: bool,
    state: Vec<Vec<Pai>>,
}

impl ShantenHelper {
    fn new(tehai: &Tehai) -> Self {
        // collect all pais in tehai and fuuros
        let mut pais: [i32; 48] = [0i32; 48];
        let mut num_pais = 0;
        for pai in tehai.view().iter() {
            // Pai.as_unify_u8 has handled 0m 0p 0s => 5m 5p 5s
            num_pais += 1;
            pais[pai.as_unify_u8() as usize] += 1;
        }
        // assert!(num_pais == 14 || num_pais == 13);
        Self {
            pais,
            num_pais_rem: num_pais,
            num_tehai: num_pais,
            verbose: true,
            state: Vec::new(),
        }
    }

    fn get_shanten(&mut self) -> i32 {
        let kokushi = self.get_kokushi_shanten();
        let chiitoi = self.get_chiitoi_shanten();
        let normal = self.get_normal_shanten();
        std::cmp::min(std::cmp::min(kokushi, chiitoi), normal)
    }

    // Get kokushi
    fn get_kokushi_shanten(&self) -> i32 {
        if self.num_tehai < 13 {
            return 8;
        }
        let mut num_kind = 0;
        let mut exist_pair = false;
        self.pais.iter().enumerate().for_each(|(idx, num)| {
            if *num == 0 {
                return;
            }
            if let Ok(pai) = Pai::try_from(idx as u8) {
                num_kind += match pai {
                    Pai::East
                    | Pai::South
                    | Pai::West
                    | Pai::North
                    | Pai::Chun
                    | Pai::Haku
                    | Pai::Hatsu
                    | Pai::Man1
                    | Pai::Man9
                    | Pai::Pin1
                    | Pai::Pin9
                    | Pai::Sou1
                    | Pai::Sou9 => {
                        if *num > 1 {
                            exist_pair = true;
                        }
                        1
                    }
                    _ => 0,
                };
            }
        });
        13 - num_kind - if exist_pair { 1 } else { 0 }
    }

    // Get shanten for (7 * pair)
    fn get_chiitoi_shanten(&self) -> i32 {
        let mut shanten = 6i32; // 6 at max for chiitoi
                                // there is any fuuro, then we can not get chiitoi
        if self.num_tehai < 13 {
            return shanten;
        }
        let mut num_kind = 0;
        self.pais.iter().enumerate().for_each(|(idx, num)| {
            if let Ok(pai) = Pai::try_from(idx as u8) {
                if *num == 0 {
                    return;
                }
                if *num >= 2 {
                    shanten -= 1;
                }
                num_kind += 1;
            }
        });
        shanten += std::cmp::max(0, 7 - num_kind);
        shanten
    }

    // Get shanten for (4 * triple + 1 * eye)
    // Return shanten: i32. Range from [-1, 8].
    //    0 for tenpai
    //   -1 for ron
    fn get_normal_shanten(&mut self) -> i32 {
        log_if!(self.verbose, "num of blocks: {}", self.num_tehai);
        let mut shanten = 8i32;
        let mut c_max = 0i32;
        let k = (self.num_pais_rem - 2) / 3;
        let eye_candidates = ShantenHelper::eyes(&self.pais);
        for eye in eye_candidates {
            // try to get the shanten with this eye
            log_if!(self.verbose, "take {} as eye begin", eye);
            self.take_eye(eye);
            self.search_by_take_3(0, 11, &mut shanten, &mut c_max, k, 1, 0);
            self.rollback_pais(&[eye, eye]);
            log_if!(self.verbose, "take {} as eye done, s: {}", eye, shanten);
        }
        // try to get the shanten without eye
        log_if!(self.verbose, "take nothing as eye begin");
        self.search_by_take_3(0, 11, &mut shanten, &mut c_max, k, 0, 0);
        log_if!(self.verbose, "take nothing as eye done, s: {}", shanten);
        shanten
    }

    fn eyes(pais: &[i32; 48]) -> Vec<Pai> {
        let mut eyes = Vec::new();
        for (idx, num) in pais.iter().enumerate() {
            if *num < 2 {
                continue;
            }
            if let Ok(pai) = Pai::try_from(idx as u8) {
                eyes.push(pai);
            }
        }
        eyes
    }

    fn take<const LEN: usize>(&mut self, pais: &[Pai; LEN]) {
        self.num_pais_rem -= pais.len() as i32;
        self.state.push(pais.to_vec());
    }

    fn rollback_pais<const LEN: usize>(&mut self, pais: &[Pai; LEN]) {
        for p in pais {
            self.pais[p.as_unify_u8() as usize] += 1;
        }
        self.num_pais_rem += pais.len() as i32;
        self.state.pop();
    }

    fn next_not_zero(&self, take_idx: usize) -> usize {
        for idx in take_idx..PAIS_VEC_LEN {
            if self.pais[idx] > 0 {
                return idx;
            }
        }
        PAIS_VEC_LEN
    }

    fn take_eye(&mut self, pai: Pai) {
        self.pais[pai.as_unify_u8() as usize] -= 2;
        let eye = [pai, pai];
        self.take(&eye);
    }

    fn try_take_3(&mut self, take_idx: usize) -> Option<([Pai; 3], usize)> {
        for idx in take_idx..48 {
            let idx = idx;
            let num = self.pais[idx];
            if num < 1 {
                continue;
            }
            // try to get triplet (AAA)
            if (41 <= idx && idx <= 47) && num >= 3 {
                if let Ok(pai) = Pai::try_from(idx as u8) {
                    self.pais[idx] -= 3;
                    let meld = [pai, pai, pai];
                    self.take(&meld);
                    return Some((meld, idx));
                }
            }
            // 1~7s, 1~7p, 1~7m
            if (11 <= idx && idx < 18) || (21 <= idx && idx < 28) || (31 <= idx && idx < 38) {
                // try to get triplet (AAA)
                if num >= 3 {
                    if let Ok(pai) = Pai::try_from(idx as u8) {
                        self.pais[idx] -= 3;
                        let meld = [pai, pai, pai];
                        self.take(&meld);
                        return Some((meld, idx));
                    }
                }
                // try to get sequence (ABC)
                if self.pais[idx + 1] < 1 || self.pais[idx + 2] < 1 {
                    continue;
                }
                self.pais[idx] -= 1;
                self.pais[idx + 1] -= 1;
                self.pais[idx + 2] -= 1;
                let meld = [
                    Pai::try_from(idx as u8).unwrap(),
                    Pai::try_from((idx + 1) as u8).unwrap(),
                    Pai::try_from((idx + 2) as u8).unwrap(),
                ];
                self.take(&meld);
                return Some((meld, idx));
            }
        }
        None
    }

    fn try_take_2(&mut self, take_idx: usize) -> Option<([Pai; 2], usize)> {
        // try get pair
        for (idx, num) in self.pais.iter_mut().enumerate() {
            if idx < take_idx as usize || *num < 2 {
                continue;
            }
            if let Ok(pai) = Pai::try_from(idx as u8) {
                self.pais[idx] -= 2;
                let res = [pai, pai];
                self.take(&res);
                return Some((res, idx));
            }
        }
        // try get RYANMEN/PENCHAN/KANCHAN
        for idx in 0..38 {
            if idx < take_idx as usize || self.pais[idx] < 1 {
                continue;
            }
            // 1~8s, 1~8p, 1~8m
            if (11 <= idx && idx < 18) || (21 <= idx && idx < 28) || (31 <= idx && idx < 38) {
                if self.pais[idx + 1] > 0 {
                    // PENCHAN/RYANMEN
                    self.pais[idx] -= 1;
                    self.pais[idx + 1] -= 1;
                    let res = [
                        Pai::try_from(idx as u8).unwrap(),
                        Pai::try_from((idx + 1) as u8).unwrap(),
                    ];
                    self.take(&res);
                    return Some((res, idx));
                } else if self.pais[idx + 2] > 0 {
                    // KANCHAN
                    self.pais[idx] -= 1;
                    self.pais[idx + 2] -= 1;
                    let res = [
                        Pai::try_from(idx as u8).unwrap(),
                        Pai::try_from((idx + 2) as u8).unwrap(),
                    ];
                    self.take(&res);
                    return Some((res, idx));
                }
            }
        }
        None
    }

    fn try_take_1(&mut self, take_idx: i32) -> Option<Pai> {
        for (idx, num) in self.pais.iter_mut().enumerate() {
            if idx < take_idx as usize || *num < 1 {
                continue;
            }
            if let Ok(pai) = Pai::try_from(idx as u8) {
                self.pais[idx] -= 2;
                let res = [pai];
                self.take(&res);
                return Some(pai);
            }
        }
        None
    }

    fn search_by_take_3(
        &mut self,
        level: i32,
        begin_idx: usize,
        shanten: &mut i32,
        c_max: &mut i32,
        k: i32,
        exist_eye: i32,
        num_meld: i32,
    ) {
        log_if!(
            self.verbose,
            "entry search_by meld with i: {}, c_rem: {}, s: {}, c_max: {}",
            begin_idx,
            self.num_pais_rem,
            shanten,
            c_max
        );
        if begin_idx >= PAIS_VEC_LEN || level > self.num_pais_rem / 3 {
            self.search_by_take_2(0, shanten, c_max, k, exist_eye, num_meld, 0);
            return;
        }

        // take a meld TODO: handle AAABC
        if let Some((meld, next_search_idx)) = self.try_take_3(begin_idx) {
            log_if!(self.verbose, "take {:?} as meld begin", meld);
            self.search_by_take_3(
                level + 1,
                next_search_idx,
                shanten,
                c_max,
                k,
                exist_eye,
                num_meld + 1,
            );
            self.rollback_pais(&meld);
            log_if!(
                self.verbose,
                "take {:?} as meld done, s: {}",
                meld,
                *shanten
            );
        }
        log_if!(
            self.verbose,
            "take nothing as meld begin, idx: {}",
            begin_idx
        );
        let next_search_idx = self.next_not_zero(begin_idx + 1);
        self.search_by_take_3(
            level,
            next_search_idx,
            shanten,
            c_max,
            k,
            exist_eye,
            num_meld,
        );
        log_if!(
            self.verbose,
            "take nothing as meld done, idx: {}, s: {}",
            begin_idx,
            *shanten
        );
    }

    fn search_by_take_2(
        &mut self,
        begin_idx: usize,
        shanten: &mut i32,
        c_max: &mut i32,
        k: i32,
        exist_eye: i32,
        num_meld: i32,
        num_dazi: i32,
    ) {
        log_if!(
            self.verbose,
            "entry search_by 2 with i: {}, c_rem: {}, s: {}, c_max: {}, g: {}, gp: {}",
            begin_idx,
            self.num_pais_rem,
            shanten,
            c_max,
            num_meld,
            num_dazi
        );
        if *shanten == -1 || num_meld + num_dazi > self.num_tehai {
            log_if!(
                self.verbose,
                "search end. cur state: {:?}. cause: s: {}, {} + {} > {}",
                self.state,
                shanten,
                num_meld,
                num_dazi,
                self.num_tehai
            );
            return;
        }
        let c = 3 * num_meld + 2 * num_dazi + 2 * exist_eye;
        if self.num_pais_rem < *c_max - c {
            log_if!(
                self.verbose,
                "search end. cur state: {:?}. cause: {} < {} - {}",
                self.state,
                self.num_pais_rem,
                c_max,
                c
            );
            return;
        }
        if self.num_pais_rem == 0 {
            let penalty = num_meld + num_dazi + exist_eye - 5;
            let num_fuuros = (14 - self.num_tehai) / 3;
            let cur_s = 9 - 2 * num_meld - num_dazi - 2 * exist_eye - num_fuuros + penalty;
            *shanten = std::cmp::min(*shanten, cur_s);
            *c_max = std::cmp::max(*c_max, c);
            log_if!(
                self.verbose,
                "search end. cur state: {:?}. cause: c_rem == 0; => s: {}, c_max: {}",
                self.state,
                shanten,
                c_max,
            );
            return;
        }
        if let Some((dazi, next_search_idx)) = self.try_take_2(begin_idx) {
            log_if!(self.verbose, "take {:?} as dazi begin", dazi);
            self.search_by_take_2(
                next_search_idx,
                shanten,
                c_max,
                k,
                exist_eye,
                num_meld,
                num_dazi + 1,
            );
            self.rollback_pais(&dazi);
            log_if!(
                self.verbose,
                "take {:?} as dazi done, s: {}",
                dazi,
                *shanten
            );
        }

        for take_idx in 0..self.num_pais_rem {
            if let Some(pai) = self.try_take_1(take_idx) {
                self.search_by_take_2(
                    begin_idx + 1,
                    shanten,
                    c_max,
                    k,
                    exist_eye,
                    num_meld,
                    num_dazi,
                );
                self.rollback_pais(&[pai]);
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use convlog::mjai::*;
    use convlog::pai::get_pais_from_str;

    use super::*;

    #[test]
    fn test_chi_to_pais() {
        let f = Fuuro::Chi {
            target: 0,
            pai: Pai::Man1,
            consumed: Consumed2::from([Pai::Man2, Pai::Man3]),
        };
        let pais = f.into_pais();
        assert_eq!(pais.len(), 3);
        assert_eq!(pais[0], Pai::Man1);
        assert_eq!(pais[1], Pai::Man2);
        assert_eq!(pais[2], Pai::Man3);
    }
    #[test]
    fn test_pon_to_pais() {
        let f = Fuuro::Pon {
            target: 0,
            pai: Pai::Man1,
            consumed: Consumed2::from([Pai::Man1, Pai::Man1]),
        };
        let pais = f.into_pais();
        assert_eq!(pais.len(), 3);
        assert_eq!(pais[0], Pai::Man1);
        assert_eq!(pais[1], Pai::Man1);
        assert_eq!(pais[2], Pai::Man1);
    }
    #[test]
    fn test_kan_to_pais() {
        let cases = [
            Fuuro::Daiminkan {
                target: 0,
                pai: Pai::Man1,
                consumed: Consumed3::from([Pai::Man1, Pai::Man1, Pai::Man1]),
            },
            Fuuro::Kakan {
                pai: Pai::Man1,
                previous_pon_target: 0,
                previous_pon_pai: Pai::Man1,
                consumed: Consumed2::from([Pai::Man1, Pai::Man1]),
            },
            Fuuro::Ankan {
                consumed: Consumed4::from([Pai::Man1, Pai::Man1, Pai::Man1, Pai::Man1]),
            },
        ];
        for case in cases {
            let pais = case.into_pais();
            assert_eq!(pais.len(), 4);
            assert_eq!(pais[0], Pai::Man1);
            assert_eq!(pais[1], Pai::Man1);
            assert_eq!(pais[2], Pai::Man1);
            assert_eq!(pais[3], Pai::Man1);
        }
    }

    enum Case {
        Normal { i: &'static str, s: i32 },
        Chiitoi { i: &'static str, s: i32 },
        Kokushi { i: &'static str, s: i32 },
    }
    #[test]
    fn test_iter_stat_pais() {
        let case: Vec<Case> = Vec::<Case>::from([
            Case::Normal {
                i: "40m12356p4699s222z",
                s: 1,
            },
            // Case::Normal {
            //     i: "0m12356p4699s4m",
            //     s: 1,
            // },
            // Case::Normal {
            //     i: "123456789p123s55m",
            //     s: -1,
            // },
            // Case::Normal {
            //     i: "12345678p123s55m1z",
            //     s: 0,
            // },
            // Case::Normal {
            //     i: "12345678p12s55m12z",
            //     s: 1,
            // },
            // Case::Normal {
            //     i: "0m125p1469s24z6p",
            //     s: 3,
            // },
            // Case::Normal {
            //     i: "0m1256p469s24z9s",
            //     s: 2,
            // },
            // Case::Normal {
            //     i: "0m1256p4699s4z3p",
            //     s: 1,
            // },
            // Case::Normal {
            //     i: "245m12356p99s222z4p",
            //     s: 0,
            // },
            // Case::Normal {
            //     i: "45m123456p99s222z2m",
            //     s: 0,
            // },
            // Case::Normal {
            //     i: "45m123456p99s222z3m",
            //     s: -1,
            // },
            // Case::Normal {
            //     i: "45m235678p399s22z6s",
            //     s: 2,
            // },
            // Case::Normal {
            //     i: "45m23568p3699s22z4m",
            //     s: 3,
            // },
            // Case::Normal {
            //     i: "445m2358p23469s2z9m",
            //     s: 4,
            // },
            // Case::Normal {
            //     i: "49m2358p23469s24z1m",
            //     s: 5,
            // },
            // Case::Normal {
            //     i: "149m258p2369s124z7s",
            //     s: 6,
            // },
            // Case::Kokushi {
            //     i: "159m19p19s1234677z",
            //     s: 0,
            // },
            // Case::Kokushi {
            //     i: "159m19p19s1236677z",
            //     s: 1,
            // },
            // Case::Chiitoi {
            //     i: "458m666p116688s55z",
            //     s: 1, // normal 2
            // },
            // Case::Chiitoi {
            //     i: "44m6666p116688s55z",
            //     s: 1, // normal 2
            // },
            // Case::Chiitoi {
            //     i: "4444m6666p1111s55z",
            //     s: 5,
            // },
            // Case::Normal {
            //     i: "4444m6666p1111s55z",
            //     s: 1,
            // },
        ]);
        for c in case {
            match c {
                Case::Normal { i: input, .. }
                | Case::Kokushi { i: input, .. }
                | Case::Chiitoi { i: input, .. } => {
                    println!("input: '{}'", input);
                    let mut helper =
                        ShantenHelper::new(&Tehai::from(get_pais_from_str(input).unwrap()));
                    match c {
                        Case::Normal { i: input, s } => {
                            let normal = helper.get_normal_shanten();
                            println!("shanten: {} for '{}'(normal)", s, input);
                            assert_eq!(s, normal, "for '{}'", input);
                        }
                        Case::Kokushi { i: input, s } => {
                            let kokushi = helper.get_kokushi_shanten();
                            println!("shanten: {} for '{}'(kokushi)", s, input);
                            assert_eq!(s, kokushi, "for '{}'", input);
                        }
                        Case::Chiitoi { i: input, s } => {
                            let chiitoi = helper.get_chiitoi_shanten();
                            println!("shanten: {} for '{}'(chiitoi)", s, input);
                            assert_eq!(s, chiitoi, "for '{}'", input);
                        }
                    }
                }
            }
        }
    }
}
