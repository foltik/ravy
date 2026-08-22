use std::sync::Arc;

use lib::prelude::*;

use super::{Energy, Look, Palette, Special, State};
use crate::mirror::Pad;

///////////////////////// BINDINGS /////////////////////////

pub struct PadBinding {
    pub xy: (i8, i8),
    pub op: PadOp,
}

#[derive(Clone)]
pub enum PadOp {
    Energy(Energy),
    /// The whole base look: energy, movement and how fast it runs.
    Look(Look),
    Palette(Box<dyn Palette>),
    Special(Special),
    Beat {
        side: usize,
        pd: Pd,
    },
    Func {
        name: String,
        /// What the button lights as, from the current state.
        lamp: Arc<dyn Fn(&State) -> Rgbw + Send + Sync + 'static>,
        func: Arc<dyn Fn(&mut State, &mut Pad) + Send + Sync + 'static>,
    },
}

impl PadOp {
    /// What this button does, for the mirror's tooltips.
    pub fn name(&self) -> String {
        match self {
            PadOp::Energy(e) => format!("energy: {}", energy_name(e)),
            PadOp::Look(look) => match (look.energy, look.movement) {
                (Some(e), Some(m)) => format!("{} {m:?}", energy_name(&e)),
                (Some(e), None) => energy_name(&e),
                _ => "look".into(),
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

fn energy_name(e: &Energy) -> String {
    match e {
        Energy::Off => "off".into(),
        Energy::On => "on".into(),
        Energy::Beat { pd } => format!("beat {pd:?}"),
        Energy::Strobe { pd, .. } => format!("strobe {pd:?}"),
        Energy::Chase { pd } => format!("chase {pd:?}"),
        Energy::Swell { pd } => format!("swell {pd:?}"),
        Energy::Alternate { pd } => format!("alternate {pd:?}"),
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
macro_rules! look {
    ($v:expr) => {
        $crate::logic::PadOp::Look($v)
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
    ($name:expr, lamp: $lamp:expr, $func:expr) => {
        $crate::logic::PadOp::Func {
            name: ($name).into(),
            lamp: std::sync::Arc::new($lamp),
            func: std::sync::Arc::new($func),
        }
    };
    ($name:expr, $color:expr, $func:expr) => {
        $crate::func!($name, lamp: move |_: &$crate::logic::State| $color, $func)
    };
}
