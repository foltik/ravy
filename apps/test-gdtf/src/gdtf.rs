//! Minimal GDTF reader: geometry tree, part meshes, and DMX modes.
//!
//! Only what's needed to drive a moving head: pan/tilt axes, beam geometries,
//! and the linear DMX -> physical mapping of each channel function.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use bevy::math::{Mat4, Vec3, Vec4};

pub struct Gdtf {
    pub name: String,
    pub manufacturer: String,
    pub models: HashMap<String, Model>,
    /// Top-level geometries. `GeometryReference` targets are always found here.
    pub geometries: Vec<Geometry>,
    pub modes: Vec<Mode>,
    /// Part meshes in .glb format, keyed by asset name.
    pub meshes: HashMap<String, Vec<u8>>,
    /// Gobo images, keyed by asset name.
    pub images: HashMap<String, Vec<u8>>,
    /// Colour/gobo/prism wheels, keyed by name.
    pub wheels: HashMap<String, Vec<Slot>>,
    pub rdm: Option<Rdm>,
}

pub struct Slot {
    pub name: String,
    /// CIE xyY, where Y is percent transmission.
    pub color: Option<Vec3>,
    /// Asset name of this slot's gobo image, if it has one.
    pub media: Option<String>,
}

/// RDM identity, for matching a discovered responder to this fixture type.
pub struct Rdm {
    pub manufacturer: u16,
    pub model: u16,
    /// Mode name -> RDM personality number.
    pub personalities: HashMap<String, u8>,
}

pub struct Model {
    /// Asset name of the .glb, if this model ships a mesh.
    pub mesh: Option<String>,
    /// Bounding box in meters, along GDTF axes (x = length, y = width, z = height).
    pub size: Vec3,
}

pub struct Geometry {
    pub name: String,
    pub kind: Kind,
    pub model: Option<String>,
    /// Transform relative to the parent, in GDTF (z-up, beam along -z) space.
    pub transform: Mat4,
    pub children: Vec<Geometry>,
}

pub enum Kind {
    Normal,
    Axis,
    Beam(Beam),
    /// Instance of a top-level geometry, by name.
    Reference(String),
}

pub struct Beam {
    /// Full cone angle in degrees, to half peak intensity.
    pub angle: f32,
    /// Full cone angle in degrees, to 10% peak intensity.
    pub field: f32,
    pub radius: f32,
    /// Luminous flux in lumens.
    pub flux: f32,
    /// A surface that lights up but casts no beam.
    pub glow: bool,
}

pub struct Mode {
    pub name: String,
    pub channels: Vec<Channel>,
}

pub struct Channel {
    pub geometry: String,
    pub attribute: String,
    /// 1-based DMX slots, coarse first.
    pub offsets: Vec<usize>,
    pub default: u32,
    pub functions: Vec<Function>,
}

pub struct Function {
    pub name: String,
    pub attribute: String,
    /// First DMX value of this function, at the channel's resolution.
    pub from: u32,
    pub physical: (f32, f32),
    /// Wheel this function selects from, if any.
    pub wheel: Option<String>,
    /// (first DMX value, 1-based wheel slot). Slot 0 means no slot, which is
    /// how the continuous-rotation ranges are written.
    pub sets: Vec<(u32, u32)>,
}

impl Gdtf {
    pub fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        let mut zip = zip::ZipArchive::new(file)?;

        let mut xml = String::new();
        let mut meshes = HashMap::new();
        let mut images = HashMap::new();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let name = entry.name().to_string();
            if name == "description.xml" {
                entry.read_to_string(&mut xml)?;
            } else if name.starts_with("models/gltf/") && name.ends_with(".glb") {
                let stem = Path::new(&name).file_stem().unwrap().to_string_lossy();
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                meshes.insert(asset_name(&stem), buf);
            } else if name.starts_with("wheels/") && name.ends_with(".png") {
                let stem = Path::new(&name).file_stem().unwrap().to_string_lossy();
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                images.insert(image_name(&stem), buf);
            }
        }
        if xml.is_empty() {
            return Err(anyhow!("no description.xml in {}", path.display()));
        }

        let doc = roxmltree::Document::parse(&xml)?;
        let ft = doc
            .descendants()
            .find(|n| n.has_tag_name("FixtureType"))
            .ok_or_else(|| anyhow!("no FixtureType"))?;

        let mut models = HashMap::new();
        for m in ft.children().filter(|n| n.has_tag_name("Models")).flat_map(|n| n.children()) {
            if !m.has_tag_name("Model") {
                continue;
            }
            let file = m.attribute("File").unwrap_or_default();
            let mesh = match file.is_empty() {
                true => None,
                false => Some(asset_name(file)),
            };
            models.insert(
                m.attribute("Name").unwrap_or_default().to_string(),
                Model {
                    mesh: mesh.filter(|name| meshes.contains_key(name)),
                    size: Vec3::new(attr_f32(m, "Length"), attr_f32(m, "Width"), attr_f32(m, "Height")),
                },
            );
        }

        let geometries = ft
            .children()
            .find(|n| n.has_tag_name("Geometries"))
            .ok_or_else(|| anyhow!("no Geometries"))?
            .children()
            .filter_map(parse_geometry)
            .collect();

        let modes = ft
            .children()
            .filter(|n| n.has_tag_name("DMXModes"))
            .flat_map(|n| n.children())
            .filter(|n| n.has_tag_name("DMXMode"))
            .map(parse_mode)
            .collect();

        let mut wheels = HashMap::new();
        for w in ft.children().filter(|n| n.has_tag_name("Wheels")).flat_map(|n| n.children()) {
            if !w.has_tag_name("Wheel") {
                continue;
            }
            let slots = w
                .children()
                .filter(|s| s.has_tag_name("Slot"))
                .map(|s| Slot {
                    name: s.attribute("Name").unwrap_or_default().trim().to_string(),
                    color: s.attribute("Color").and_then(parse_cie),
                    media: s
                        .attribute("MediaFileName")
                        .filter(|m| !m.is_empty())
                        .map(|m| image_name(m))
                        .filter(|m| images.contains_key(m)),
                })
                .collect();
            wheels.insert(w.attribute("Name").unwrap_or_default().to_string(), slots);
        }

        let rdm = ft.descendants().find(|n| n.has_tag_name("FTRDM")).map(|n| Rdm {
            manufacturer: attr_hex(n, "ManufacturerID"),
            model: attr_hex(n, "DeviceModelID"),
            personalities: n
                .descendants()
                .filter(|p| p.has_tag_name("DMXPersonality"))
                .filter_map(|p| {
                    let mode = p.attribute("DMXMode")?.to_string();
                    Some((mode, attr_hex(p, "Value") as u8))
                })
                .collect(),
        });

        Ok(Self {
            name: ft.attribute("LongName").or(ft.attribute("Name")).unwrap_or("?").to_string(),
            manufacturer: ft.attribute("Manufacturer").unwrap_or_default().to_string(),
            models,
            geometries,
            modes,
            meshes,
            images,
            wheels,
            rdm,
        })
    }

    pub fn find(&self, name: &str) -> Option<&Geometry> {
        fn search<'a>(geometries: &'a [Geometry], name: &str) -> Option<&'a Geometry> {
            for g in geometries {
                if g.name == name {
                    return Some(g);
                }
                if let Some(found) = search(&g.children, name) {
                    return Some(found);
                }
            }
            None
        }
        search(&self.geometries, name)
    }

    /// Names of a geometry and everything below it.
    pub fn subtree(&self, name: &str) -> Vec<String> {
        fn collect(g: &Geometry, out: &mut Vec<String>) {
            out.push(g.name.clone());
            for c in &g.children {
                collect(c, out);
            }
        }
        let mut out = vec![];
        if let Some(g) = self.find(name) {
            collect(g, &mut out);
        }
        out
    }
}

impl Mode {
    /// Number of DMX slots this mode occupies.
    pub fn footprint(&self) -> usize {
        self.channels.iter().flat_map(|c| c.offsets.iter().copied()).max().unwrap_or(0)
    }

    /// Pack channel values into a footprint-sized DMX frame.
    pub fn encode(&self, values: &[u32]) -> Vec<u8> {
        let mut frame = vec![0u8; self.footprint()];
        for (channel, &value) in self.channels.iter().zip(values) {
            let bytes = channel.offsets.len();
            for (i, &slot) in channel.offsets.iter().enumerate() {
                frame[slot - 1] = (value >> (8 * (bytes - 1 - i))) as u8;
            }
        }
        frame
    }

    /// Index of the channel driving `attribute` for a geometry, given its
    /// ancestor chain (the geometry itself last).
    ///
    /// Falls back to searching each ancestor's subtree, since modes without
    /// per-pixel control attach a whole group's color to one representative
    /// geometry (e.g. the outcast's ring color lives on `Beam_1`).
    pub fn resolve(&self, gdtf: &Gdtf, chain: &[String], attribute: &str) -> Option<usize> {
        let find = |pred: &dyn Fn(&Channel) -> bool| self.channels.iter().position(pred);

        for name in chain.iter().rev() {
            if let Some(i) = find(&|c| c.attribute == attribute && &c.geometry == name) {
                return Some(i);
            }
        }
        for name in chain.iter().rev() {
            let subtree = gdtf.subtree(name);
            if let Some(i) = find(&|c| c.attribute == attribute && subtree.contains(&c.geometry)) {
                return Some(i);
            }
        }
        None
    }
}

impl Channel {
    pub fn max(&self) -> u32 {
        match self.offsets.len() {
            0 | 1 => u8::MAX as u32,
            2 => u16::MAX as u32,
            _ => u32::MAX,
        }
    }

    pub fn function(&self, value: u32) -> &Function {
        let i = self.functions.iter().rposition(|f| f.from <= value).unwrap_or(0);
        &self.functions[i]
    }

    /// Physical value at `value`, interpolated across the active function's range.
    pub fn physical(&self, value: u32) -> f32 {
        let i = self.functions.iter().rposition(|f| f.from <= value).unwrap_or(0);
        let f = &self.functions[i];
        let to = self.functions.get(i + 1).map_or(self.max(), |next| next.from.saturating_sub(1));
        let t = match to > f.from {
            true => (value.min(to) - f.from) as f32 / (to - f.from) as f32,
            false => 0.0,
        };
        f.physical.0 + t * (f.physical.1 - f.physical.0)
    }

    /// Wheel and 0-based slot selected at `value`, if this channel picks one.
    /// The continuous-rotation ranges select no slot.
    pub fn slot(&self, value: u32) -> Option<(&str, usize)> {
        let function = self.function(value);
        let wheel = function.wheel.as_deref()?;
        let i = function.sets.iter().rposition(|(from, _)| *from <= value)?;
        let slot = function.sets[i].1;
        (slot > 0).then(|| (wheel, slot as usize - 1))
    }

    pub fn label(&self) -> String {
        let slots = self
            .offsets
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join("/");
        format!("{slots}  {} ({})", self.attribute, self.geometry)
    }
}

fn parse_geometry(n: roxmltree::Node) -> Option<Geometry> {
    let kind = match n.tag_name().name() {
        "Axis" => Kind::Axis,
        "Geometry" | "Support" | "Structure" | "Inventory" => Kind::Normal,
        "Beam" => Kind::Beam(Beam {
            angle: attr_f32(n, "BeamAngle"),
            field: attr_f32(n, "FieldAngle"),
            radius: attr_f32(n, "BeamRadius"),
            flux: attr_f32(n, "LuminousFlux"),
            glow: n.attribute("BeamType") == Some("Glow"),
        }),
        "GeometryReference" => Kind::Reference(n.attribute("Geometry")?.to_string()),
        _ => return None,
    };

    Some(Geometry {
        name: n.attribute("Name").unwrap_or_default().to_string(),
        kind,
        model: n.attribute("Model").map(str::to_string).filter(|m| !m.is_empty()),
        transform: parse_matrix(n.attribute("Position").unwrap_or_default()),
        children: n.children().filter_map(parse_geometry).collect(),
    })
}

fn parse_mode(n: roxmltree::Node) -> Mode {
    let channels = n
        .children()
        .filter(|c| c.has_tag_name("DMXChannels"))
        .flat_map(|c| c.children())
        .filter(|c| c.has_tag_name("DMXChannel"))
        .filter_map(|c| {
            let offsets: Vec<usize> = c
                .attribute("Offset")
                .unwrap_or_default()
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if offsets.is_empty() {
                return None; // virtual channel, nothing to drive it with
            }

            let logical = c.children().find(|l| l.has_tag_name("LogicalChannel"))?;
            let bytes = offsets.len() as u32;
            let mut default = 0;
            let mut functions = vec![];
            for (i, f) in logical.children().filter(|f| f.has_tag_name("ChannelFunction")).enumerate() {
                if i == 0 {
                    default = parse_dmx(f.attribute("Default").unwrap_or_default(), bytes);
                }
                functions.push(Function {
                    name: f.attribute("Name").unwrap_or_default().to_string(),
                    attribute: f.attribute("Attribute").unwrap_or_default().to_string(),
                    from: parse_dmx(f.attribute("DMXFrom").unwrap_or_default(), bytes),
                    physical: (attr_f32(f, "PhysicalFrom"), attr_f32(f, "PhysicalTo")),
                    wheel: f.attribute("Wheel").map(str::to_string).filter(|w| !w.is_empty()),
                    sets: f
                        .children()
                        .filter(|s| s.has_tag_name("ChannelSet"))
                        .map(|s| {
                            (
                                parse_dmx(s.attribute("DMXFrom").unwrap_or_default(), bytes),
                                s.attribute("WheelSlotIndex").and_then(|v| v.parse().ok()).unwrap_or(0),
                            )
                        })
                        .collect(),
                });
            }
            if functions.is_empty() {
                return None;
            }

            Some(Channel {
                geometry: c.attribute("Geometry").unwrap_or_default().to_string(),
                attribute: logical.attribute("Attribute").unwrap_or_default().to_string(),
                offsets,
                default,
                functions,
            })
        })
        .collect();

    Mode { name: n.attribute("Name").unwrap_or_default().to_string(), channels }
}

/// Parse a `value/bytes` DMX literal, rescaled to `bytes` resolution.
fn parse_dmx(s: &str, bytes: u32) -> u32 {
    let (value, from) = s.split_once('/').unwrap_or((s, "1"));
    let value: u64 = value.trim().parse().unwrap_or(0);
    let from: u32 = from.trim().parse().unwrap_or(1);
    let shifted = match from.cmp(&bytes) {
        std::cmp::Ordering::Less => value << (8 * (bytes - from)),
        std::cmp::Ordering::Greater => value >> (8 * (from - bytes)),
        std::cmp::Ordering::Equal => value,
    };
    shifted as u32
}

/// Parse a GDTF `Position` matrix. Each brace holds a basis vector followed by
/// one component of the translation.
fn parse_matrix(s: &str) -> Mat4 {
    let n: Vec<f32> = s
        .split(|c: char| c == '{' || c == '}' || c == ',')
        .filter_map(|t| t.trim().parse().ok())
        .collect();
    if n.len() < 16 {
        return Mat4::IDENTITY;
    }
    Mat4::from_cols(
        Vec4::new(n[0], n[1], n[2], 0.0),
        Vec4::new(n[4], n[5], n[6], 0.0),
        Vec4::new(n[8], n[9], n[10], 0.0),
        Vec4::new(n[3], n[7], n[11], 1.0),
    )
}

/// Parse a GDTF `x,y,Y` colour.
fn parse_cie(s: &str) -> Option<Vec3> {
    let v: Vec<f32> = s.split(',').filter_map(|t| t.trim().parse().ok()).collect();
    (v.len() >= 3).then(|| Vec3::new(v[0], v[1], v[2]))
}

fn attr_hex(n: roxmltree::Node, name: &str) -> u16 {
    let v = n.attribute(name).unwrap_or_default();
    u16::from_str_radix(v.trim_start_matches("0x").trim_start_matches("0X"), 16).unwrap_or(0)
}

fn attr_f32(n: roxmltree::Node, name: &str) -> f32 {
    n.attribute(name).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

fn image_name(file: &str) -> String {
    let stem: String = file
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{stem}.png")
}

/// GDTF model names are free-form, so squash them into a safe asset path.
fn asset_name(file: &str) -> String {
    let stem: String = file
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{stem}.glb")
}
