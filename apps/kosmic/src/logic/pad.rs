use std::sync::Arc;

use lib::prelude::*;

use super::{Energy, Palette, Special, State};
use crate::mirror::Pad;

///////////////////////// BINDINGS /////////////////////////

pub struct PadBinding {
    pub xy: (i8, i8),
    pub op: PadOp,
}

#[derive(Clone)]
pub enum PadOp {
    Energy(Energy),
    Palette(Box<dyn Palette>),
    Special(Special),
    Beat {
        side: usize,
        pd: Pd,
    },
    Func {
        name: &'static str,
        color: Rgbw,
        func: Arc<dyn Fn(&mut State, &mut Pad) + Send + Sync + 'static>,
    },
}

impl PadOp {
    /// What this button does, for the mirror's tooltips.
    pub fn name(&self) -> String {
        match self {
            PadOp::Energy(e) => match e {
                Energy::Off => "energy: off".into(),
                Energy::On => "energy: on".into(),
                Energy::Beat { pd } => format!("energy: beat {pd:?}"),
                Energy::Strobe { pd, .. } => format!("energy: strobe {pd:?}"),
                Energy::Chase { pd } => format!("energy: chase {pd:?}"),
            },
            PadOp::Palette(p) => format!("palette: {}", p.name()),
            PadOp::Special(s) => format!("special: {}", s.name),
            PadOp::Beat { side, pd } => {
                format!("beat {} {pd:?}", if *side == 0 { "left" } else { "right" })
            }
            PadOp::Func { name, .. } => name.to_string(),
        }
    }
}

///////////////////////// MACROS /////////////////////////

#[macro_export]
macro_rules! bind {
    ( $name:ident: $( ($x:expr, $y:expr) => $op:expr),* $(,)? ) => {
        pub fn $name() -> Vec<$crate::logic::PadBinding> {
            vec![
                $(
                    $crate::logic::PadBinding {
                        xy: ($x, $y),
                        op: $op,
                    }
                ),*
            ]
        }
    };
}

#[macro_export]
macro_rules! energy {
    ($($v:tt)*) => {
        $crate::logic::PadOp::Energy($crate::logic::Energy::$($v)*)
    };
}

#[macro_export]
macro_rules! palette {
    ($v:expr) => {
        $crate::logic::PadOp::Palette(Box::new($v))
    };
}

#[macro_export]
macro_rules! special {
    ($v:expr) => {
        $crate::logic::PadOp::Special($v)
    };
}

#[macro_export]
macro_rules! beat {
    ($side:expr, $pd:expr) => {
        $crate::logic::PadOp::Beat { side: $side, pd: $pd }
    };
}

#[macro_export]
macro_rules! func {
    ($name:expr, $color:expr, $func:expr) => {
        $crate::logic::PadOp::Func {
            name: $name,
            color: $color,
            func: std::sync::Arc::new($func),
        }
    };
}
