//! Decoupage d'un Dockerfile en instructions.
//!
//! Trois subtilites justifient un lexer dedie plutot qu'un simple `lines()` :
//! les continuations de ligne par contre-oblique, les commentaires qui peuvent
//! apparaitre *au milieu* d'une continuation (Docker les retire), et les
//! blocs heredoc de la syntaxe recente.

/// Une instruction Dockerfile, continuations deja resolues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Mot-cle en majuscules : `FROM`, `RUN`, `ENV`...
    pub keyword: String,
    /// Tout ce qui suit le mot-cle, sur une seule ligne logique.
    pub args: String,
    /// Ligne du fichier source, pour citer la preuve dans un `Finding`.
    pub line: u32,
}

pub fn lex(content: &str) -> Vec<Instruction> {
    let mut out = Vec::new();
    let mut lines = content.lines().enumerate().peekable();

    while let Some((idx, raw)) = lines.next() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let start_line = idx as u32 + 1;
        let mut logical = strip_continuation(trimmed).to_string();

        // Tant que la ligne se termine par une contre-oblique, on agrege la
        // suivante. Les commentaires intercales sont ignores, comme le fait Docker.
        let mut continues = ends_with_continuation(trimmed);
        while continues {
            match lines.next() {
                None => break,
                Some((_, next_raw)) => {
                    let next = next_raw.trim();
                    if next.starts_with('#') {
                        continue;
                    }
                    logical.push(' ');
                    logical.push_str(strip_continuation(next));
                    continues = ends_with_continuation(next);
                }
            }
        }

        let (keyword, args) = match logical.split_once(char::is_whitespace) {
            Some((k, a)) => (k.to_ascii_uppercase(), a.trim().to_string()),
            None => (logical.to_ascii_uppercase(), String::new()),
        };

        // Un heredoc (`RUN <<EOF`) : on avale le corps et on le rend comme
        // arguments, sinon les lignes suivantes seraient lues comme des
        // instructions Dockerfile.
        let args = match heredoc_tag(&args) {
            None => args,
            Some(tag) => {
                let mut body = Vec::new();
                for (_, l) in lines.by_ref() {
                    if l.trim() == tag {
                        break;
                    }
                    body.push(l.trim().to_string());
                }
                body.join(" && ")
            }
        };

        out.push(Instruction {
            keyword,
            args,
            line: start_line,
        });
    }
    out
}

fn ends_with_continuation(line: &str) -> bool {
    line.ends_with('\\')
}

fn strip_continuation(line: &str) -> &str {
    line.strip_suffix('\\').unwrap_or(line).trim_end()
}

/// Rend le marqueur de fin d'un heredoc, pour `RUN <<EOF` ou `RUN <<-"EOF"`.
fn heredoc_tag(args: &str) -> Option<String> {
    let rest = args.trim().strip_prefix("<<")?;
    let rest = rest.strip_prefix('-').unwrap_or(rest);
    let tag: String = rest
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .split_whitespace()
        .next()?
        .to_string();
    (!tag.is_empty()).then_some(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kw(instrs: &[Instruction]) -> Vec<&str> {
        instrs.iter().map(|i| i.keyword.as_str()).collect()
    }

    #[test]
    fn les_commentaires_et_lignes_vides_sont_ignores() {
        let i = lex("# un commentaire\n\nFROM debian\n");
        assert_eq!(kw(&i), vec!["FROM"]);
        assert_eq!(i[0].args, "debian");
    }

    #[test]
    fn une_continuation_agrege_les_lignes_en_une_instruction() {
        let i = lex("RUN apt-get update \\\n && apt-get install -y \\\n    curl git\n");
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].args, "apt-get update && apt-get install -y curl git");
    }

    #[test]
    fn un_commentaire_au_milieu_d_une_continuation_disparait() {
        let i = lex("RUN foo \\\n# explication\n  bar\n");
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].args, "foo bar");
    }

    #[test]
    fn le_numero_de_ligne_pointe_le_debut_de_l_instruction() {
        let i = lex("# c\nFROM debian\nRUN a \\\n  b\nCMD x\n");
        assert_eq!(i[0].line, 2);
        assert_eq!(i[1].line, 3);
        assert_eq!(i[2].line, 5);
    }

    #[test]
    fn le_corps_d_un_heredoc_ne_devient_pas_des_instructions() {
        let i = lex("RUN <<EOF\napt-get update\napt-get install -y curl\nEOF\nCMD [\"x\"]\n");
        assert_eq!(kw(&i), vec!["RUN", "CMD"]);
        assert_eq!(i[0].args, "apt-get update && apt-get install -y curl");
    }

    #[test]
    fn le_mot_cle_est_normalise_en_majuscules() {
        let i = lex("from debian:bookworm\n");
        assert_eq!(i[0].keyword, "FROM");
    }
}
