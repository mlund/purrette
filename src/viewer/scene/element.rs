//! Chemical elements the viewer distinguishes, resolved once per atom so radii and
//! colours are table lookups rather than string matches in hot loops.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Element {
    #[default]
    Other,
    H,
    C,
    N,
    O,
    S,
    P,
    Se,
    Zn,
    Fe,
    Cu,
    Mn,
    Co,
    Ni,
    Mg,
    Ca,
    Na,
    K,
}

const SYMBOLS: [(&str, Element); 17] = [
    ("H", Element::H),
    ("C", Element::C),
    ("N", Element::N),
    ("O", Element::O),
    ("S", Element::S),
    ("P", Element::P),
    ("SE", Element::Se),
    ("ZN", Element::Zn),
    ("FE", Element::Fe),
    ("CU", Element::Cu),
    ("MN", Element::Mn),
    ("CO", Element::Co),
    ("NI", Element::Ni),
    ("MG", Element::Mg),
    ("CA", Element::Ca),
    ("NA", Element::Na),
    ("K", Element::K),
];

impl Element {
    pub fn parse(symbol: &str) -> Self {
        SYMBOLS
            .iter()
            .find(|(s, _)| s.eq_ignore_ascii_case(symbol.trim()))
            .map_or(Self::Other, |&(_, e)| e)
    }

    /// Bondi (1964) radii, Å; others 1.8 Å, a typical heavy-atom size.
    pub fn vdw_radius(self) -> f32 {
        match self {
            Self::H => 1.20,
            Self::C => 1.70,
            Self::N => 1.55,
            Self::O => 1.52,
            Self::S | Self::P => 1.80,
            Self::Se => 1.90,
            Self::Zn => 1.39,
            Self::Cu => 1.40,
            Self::Mg => 1.73,
            Self::Na => 2.27,
            Self::K => 2.75,
            _ => 1.80,
        }
    }

    /// Covalent radii (Å), Cordero et al. 2008; others 0.75 Å, about a light heavy atom.
    pub fn covalent_radius(self) -> f32 {
        match self {
            Self::H => 0.31,
            Self::C => 0.76,
            Self::N => 0.71,
            Self::O => 0.66,
            Self::S => 1.05,
            Self::P => 1.07,
            Self::Se => 1.20,
            _ => 0.75,
        }
    }

    /// Largest covalent radius in the table, bounding any bond length.
    pub const MAX_COVALENT_RADIUS: f32 = 1.20;

    /// Jmol-like CPK colours, the convention most viewers share.
    pub fn color(self) -> [f32; 3] {
        match self {
            Self::H => [0.95, 0.95, 0.95],
            Self::C => [0.56, 0.56, 0.56],
            Self::N => [0.19, 0.31, 0.97],
            Self::O => [1.00, 0.05, 0.05],
            Self::S => [1.00, 0.78, 0.20],
            Self::P => [1.00, 0.50, 0.00],
            Self::Se => [1.00, 0.63, 0.00],
            Self::Zn | Self::Fe | Self::Cu | Self::Mn | Self::Co | Self::Ni => [0.50, 0.50, 0.78],
            Self::Mg | Self::Ca | Self::Na | Self::K => [0.24, 1.00, 0.00],
            Self::Other => [1.00, 0.08, 0.58],
        }
    }
}
