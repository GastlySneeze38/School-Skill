//! Chiffrement des échanges avec Pronote (repris de pronotepy, lui-même repris de eleve.js) :
//! AES-128-CBC avec bourrage PKCS#7, clés dérivées par MD5.

use aes::Aes128;
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
use md5::{Digest, Md5};

pub fn md5(donnees: &[u8]) -> [u8; 16] {
    Md5::digest(donnees).into()
}

pub fn sha256_hex_maj(texte: &str) -> String {
    hex::encode_upper(sha2::Sha256::digest(texte.as_bytes()))
}

#[derive(Clone)]
pub struct Chiffrement {
    pub cle: [u8; 16],
    pub iv: [u8; 16],
}

impl Chiffrement {
    /// État initial d'une session : clé = MD5(""), IV nul.
    pub fn initial() -> Self {
        Chiffrement { cle: md5(b""), iv: [0; 16] }
    }

    pub fn chiffrer(&self, donnees: &[u8]) -> Vec<u8> {
        cbc::Encryptor::<Aes128>::new(&self.cle.into(), &self.iv.into()).encrypt_padded_vec::<Pkcs7>(donnees)
    }

    pub fn dechiffrer(&self, donnees: &[u8]) -> anyhow::Result<Vec<u8>> {
        cbc::Decryptor::<Aes128>::new(&self.cle.into(), &self.iv.into())
            .decrypt_padded_vec::<Pkcs7>(donnees)
            .map_err(|_| anyhow::anyhow!("déchiffrement Pronote impossible (mauvaise clé : identifiants incorrects ?)"))
    }
}

/// Bourrage PKCS#7 seul (Pronote l'applique deux fois pour les URL de fichiers).
pub fn bourrer(donnees: &[u8]) -> Vec<u8> {
    let n = 16 - donnees.len() % 16;
    let mut v = donnees.to_vec();
    v.extend(std::iter::repeat_n(n as u8, n));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour() {
        let c = Chiffrement { cle: md5(b"cle"), iv: md5(b"iv") };
        let clair = b"{\"donnees\":42}";
        assert_eq!(c.dechiffrer(&c.chiffrer(clair)).unwrap(), clair);
    }

    /// Valeurs calculées avec pycryptodome (comme pronotepy) : les octets doivent être identiques.
    #[test]
    fn identique_a_pronotepy() {
        let c = Chiffrement { cle: md5(b"cle"), iv: md5(b"iv") };
        assert_eq!(hex::encode(c.chiffrer(b"3")), "4e0fbabe4c9b8db6040c61408589773c");
        let defi = Chiffrement { cle: md5(sha256_hex_maj("secret").as_bytes()), iv: md5(b"iv") };
        assert_eq!(hex::encode(defi.chiffrer(b"challenge")), "c4a6dd535631df94b2d4321156e35dbc");
    }

    #[test]
    fn md5_vide_connu() {
        assert_eq!(hex::encode(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn bourrage_bloc_complet() {
        assert_eq!(bourrer(&[0u8; 16]).len(), 32);
        assert_eq!(bourrer(&[0u8; 5]).len(), 16);
    }
}
