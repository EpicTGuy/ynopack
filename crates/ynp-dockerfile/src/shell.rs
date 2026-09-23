//! Decoupage de lignes de commande shell.
//!
//! Les lignes `RUN` d'un Dockerfile sont du shell. On n'a pas besoin d'un
//! interpreteur : il suffit de savoir ou s'arrete une commande et quels sont
//! ses mots, en respectant les guillemets. C'est ce que fait ce module, et
//! c'est suffisant pour reconnaitre un `apt-get install` au milieu d'une chaine
//! de dix commandes.

/// Decoupe une ligne shell en commandes elementaires, sur `&&`, `||` et `;`.
///
/// Les separateurs a l'interieur de guillemets sont ignores : une commande
/// comme `sh -c "a && b"` reste une seule commande.
pub fn split_commands(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match (c, quote) {
            ('\\', _) => {
                cur.push(c);
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            ('\'' | '"', None) => {
                quote = Some(c);
                cur.push(c);
            }
            (c2, Some(q)) if c2 == q => {
                quote = None;
                cur.push(c2);
            }
            (_, Some(_)) => cur.push(c),
            (';', None) => {
                push_cmd(&mut out, &mut cur);
            }
            ('&', None) if chars.peek() == Some(&'&') => {
                chars.next();
                push_cmd(&mut out, &mut cur);
            }
            ('|', None) if chars.peek() == Some(&'|') => {
                chars.next();
                push_cmd(&mut out, &mut cur);
            }
            _ => cur.push(c),
        }
    }
    push_cmd(&mut out, &mut cur);
    out
}

fn push_cmd(out: &mut Vec<String>, cur: &mut String) {
    let t = cur.trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
    cur.clear();
}

/// Decoupe une commande en mots, en retirant les guillemets.
pub fn tokenize(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut seen = false;
    let mut chars = cmd.chars().peekable();

    while let Some(c) = chars.next() {
        match (c, quote) {
            ('\\', _) => {
                // Une contre-oblique en fin de ligne est une continuation deja
                // resolue par le lexer ; ailleurs, elle echappe le caractere suivant.
                if let Some(n) = chars.next() {
                    cur.push(n);
                    seen = true;
                }
            }
            ('\'' | '"', None) => {
                quote = Some(c);
                seen = true;
            }
            (c2, Some(q)) if c2 == q => quote = None,
            (_, Some(_)) => cur.push(c),
            (c2, None) if c2.is_whitespace() => {
                if seen {
                    out.push(std::mem::take(&mut cur));
                    seen = false;
                }
            }
            _ => {
                cur.push(c);
                seen = true;
            }
        }
    }
    if seen {
        out.push(cur);
    }
    out
}

/// Vrai si le mot contient une substitution shell : son contenu n'est pas
/// connu statiquement, on ne peut donc pas le traiter comme un nom de paquet.
pub fn is_dynamic(token: &str) -> bool {
    token.contains('$') || token.contains('`') || token.contains('*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_chaine_de_commandes_se_decoupe_sur_les_operateurs() {
        let c = split_commands("apt-get update && apt-get install -y curl ; rm -rf /tmp/x");
        assert_eq!(
            c,
            vec!["apt-get update", "apt-get install -y curl", "rm -rf /tmp/x"]
        );
    }

    #[test]
    fn les_operateurs_entre_guillemets_ne_coupent_pas() {
        let c = split_commands(r#"sh -c "a && b" && echo ok"#);
        assert_eq!(c, vec![r#"sh -c "a && b""#, "echo ok"]);
    }

    #[test]
    fn les_mots_sont_extraits_sans_leurs_guillemets() {
        assert_eq!(
            tokenize(r#"echo "hello world" 'x y'"#),
            vec!["echo", "hello world", "x y"]
        );
    }

    #[test]
    fn un_argument_vide_entre_guillemets_reste_un_argument() {
        assert_eq!(tokenize(r#"cmd "" x"#), vec!["cmd", "", "x"]);
    }

    #[test]
    fn une_substitution_shell_est_reconnue_comme_non_statique() {
        assert!(is_dynamic("$PKG"));
        assert!(is_dynamic("${NODE_VERSION}"));
        assert!(is_dynamic("`cat list`"));
        assert!(!is_dynamic("libvips-dev"));
    }
}
