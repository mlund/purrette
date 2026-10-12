//! A loaded protein structure, kept as raw bytes so it can be re-coarse-grained
//! when the policy or chain selection changes.

use std::sync::Arc;

use cgkitten::{Atom, Bead, atoms_bytes, coarse_grain_bytes, filter_chains};

use super::setup::Policy;

#[derive(Clone, Debug)]
pub struct Structure {
    name: String,
    data: Vec<u8>,
    chains: Vec<String>,
    /// Parsed once and shared: large assemblies have ~10⁵ atoms, and every
    /// recalculation and view uses them.
    atoms: Arc<[Atom]>,
}

impl Structure {
    /// Parse PDB or mmCIF (detected from content); fails if no residues are found.
    pub fn parse(name: impl Into<String>, data: Vec<u8>) -> Result<Self, String> {
        let name = name.into();
        // Chains from the atoms of amino acids (what becomes beads), so the file is
        // parsed once here; coarse-graining runs per calculation.
        let atoms: Arc<[Atom]> = atoms_bytes(&data).into();
        let mut chains: Vec<String> = atoms
            .iter()
            .filter(|a| cgkitten::residue::is_amino_acid(&a.res_name))
            .map(|a| a.chain_id.clone())
            .collect();
        if chains.is_empty() {
            return Err(format!("{name}: no protein residues found"));
        }
        chains.sort_unstable();
        chains.dedup();
        Ok(Self {
            name,
            data,
            chains,
            atoms,
        })
    }

    /// File name, e.g. `2CDS.cif`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Name without extension, for output files.
    pub fn stem(&self) -> &str {
        self.name
            .rsplit_once('.')
            .map_or(self.name.as_str(), |(stem, _)| stem)
    }

    pub fn chains(&self) -> &[String] {
        &self.chains
    }

    /// Beads of the selected chains (all when `chains` is empty).
    pub(crate) fn beads(&self, policy: Policy, chains: &[String]) -> Result<Vec<Bead>, String> {
        let beads = filter_chains(
            coarse_grain_bytes(&self.data, policy.coarse_grain()),
            chains,
        );
        if beads.is_empty() {
            return Err("no residues in the selected chains".into());
        }
        Ok(beads)
    }
}

impl Structure {
    /// All-atom records of the selected chains (all when `chains` is empty).
    pub(crate) fn atoms(&self, chains: &[String]) -> Arc<[Atom]> {
        if chains.is_empty() {
            return Arc::clone(&self.atoms);
        }
        self.atoms
            .iter()
            .filter(|a| chains.contains(&a.chain_id))
            .cloned()
            .collect()
    }
}

/// RCSB download URL for a four-character PDB ID. mmCIF, since large
/// assemblies have no legacy PDB file.
pub fn rcsb_url(id: &str) -> Result<(String, String), String> {
    let id = id.trim().to_ascii_uppercase();
    if id.len() != 4 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("'{id}' is not a four-character PDB ID"));
    }
    Ok((
        format!("https://files.rcsb.org/download/{id}.cif"),
        format!("{id}.cif"),
    ))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Two-chain fragment with a titratable Glu in each chain.
    pub const PDB: &str = "\
ATOM      1  N   ALA A   1       0.000   0.000   0.000  1.00  0.00           N
ATOM      2  CA  ALA A   1       1.458   0.000   0.000  1.00  0.00           C
ATOM      3  C   ALA A   1       2.009   1.420   0.000  1.00  0.00           C
ATOM      4  O   ALA A   1       1.251   2.390   0.000  1.00  0.00           O
ATOM      5  CB  ALA A   1       1.988  -0.773  -1.199  1.00  0.00           C
ATOM      6  N   GLU A   2       3.332   1.536   0.000  1.00  0.00           N
ATOM      7  CA  GLU A   2       3.988   2.839   0.000  1.00  0.00           C
ATOM      8  C   GLU A   2       5.504   2.693   0.000  1.00  0.00           C
ATOM      9  O   GLU A   2       6.030   1.580   0.000  1.00  0.00           O
ATOM     10  CB  GLU A   2       3.542   3.661   1.208  1.00  0.00           C
ATOM     11  CG  GLU A   2       4.043   5.095   1.208  1.00  0.00           C
ATOM     12  CD  GLU A   2       3.597   5.917   2.416  1.00  0.00           C
ATOM     13  OE1 GLU A   2       2.889   5.367   3.288  1.00  0.00           O
ATOM     14  OE2 GLU A   2       3.958   7.111   2.481  1.00  0.00           O
ATOM     15  OXT GLU A   2       6.200   3.700   0.000  1.00  0.00           O
ATOM     16  N   LYS B   1      10.000   0.000   0.000  1.00  0.00           N
ATOM     17  CA  LYS B   1      11.458   0.000   0.000  1.00  0.00           C
ATOM     18  C   LYS B   1      12.009   1.420   0.000  1.00  0.00           C
ATOM     19  O   LYS B   1      11.251   2.390   0.000  1.00  0.00           O
ATOM     20  CB  LYS B   1      11.988  -0.773  -1.199  1.00  0.00           C
ATOM     21  CG  LYS B   1      12.988  -1.773  -1.199  1.00  0.00           C
ATOM     22  CD  LYS B   1      13.988  -2.773  -1.199  1.00  0.00           C
ATOM     23  CE  LYS B   1      14.988  -3.773  -1.199  1.00  0.00           C
ATOM     24  NZ  LYS B   1      15.988  -4.773  -1.199  1.00  0.00           N
ATOM     25  OXT LYS B   1      12.700   1.420   1.000  1.00  0.00           O
END
";

    pub fn structure() -> Structure {
        Structure::parse("test.pdb", PDB.as_bytes().to_vec()).unwrap()
    }

    #[test]
    fn lists_chains() {
        assert_eq!(structure().chains(), ["A", "B"]);
    }

    #[test]
    fn chain_selection_filters_beads() {
        let s = structure();
        let all = s.beads(Policy::Multi, &[]).unwrap();
        let a = s.beads(Policy::Multi, &["A".to_string()]).unwrap();
        assert!(a.len() < all.len());
        assert!(a.iter().all(|b| b.chain_id == "A"));
        assert!(s.beads(Policy::Multi, &["Z".to_string()]).is_err());
    }

    #[test]
    fn rejects_non_structures() {
        assert!(Structure::parse("x.txt", b"hello".to_vec()).is_err());
    }

    #[test]
    fn validates_pdb_id() {
        assert_eq!(rcsb_url(" 2cds ").unwrap().1, "2CDS.cif");
        assert!(rcsb_url("2CD").is_err());
        assert!(rcsb_url("2C/S").is_err());
    }

    #[test]
    fn stem_strips_extension() {
        assert_eq!(structure().stem(), "test");
    }
}
