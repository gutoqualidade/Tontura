//! TENERURA — manifesto de sistema: que núcleo e que tarefas compõem a
//! imagem, e que capacidades cada tarefa recebe no boot.
//!
//! ```text
//! # comentário
//! kernel kernel/kernel.tas
//! task ping user/ping.tas send:0=1/7     ; slot 0: envia no endpoint 1, selo 7
//! task pong user/pong.tas recv:0=1       ; slot 0: recebe no endpoint 1
//! task log  user/log.tas  con:8=0        ; slot 8: console de depuração
//! ```
//! Opções por tarefa (depois do arquivo):
//!   send:S=E/B  recv:S=E  con:S=0  task:S=ÍNDICE  irq:S=LINHA   capacidades (S = slot)
//!   fault=E/B        avisos de morte desta tarefa vão ao endpoint E com selo B
//!   irqline=L:E/B    esta tarefa é o driver da linha L (chega como mensagem em E, selo B)
//!   dev=console@P    recebe a página do console na página virtual P
//!   stack=N          páginas de pilha
//! Caminhos relativos ao diretório do manifesto.

use std::path::Path;

use crate::asm::assemble_file;
use crate::sysimage::{build_system, CapSpec, SystemImage, TaskSpec};

pub fn parse_cap(s: &str) -> Result<CapSpec, String> {
    let bad = || format!("capacidade inválida '{s}' (esperava tipo:slot=objeto[/selo])");
    let (kind, rest) = s.split_once(':').ok_or_else(bad)?;
    let (slot, rest) = rest.split_once('=').ok_or_else(bad)?;
    let (obj, badge) = match rest.split_once('/') {
        Some((o, b)) => (o, b),
        None => (rest, "0"),
    };
    let kind = match kind {
        "send" => 1,
        "recv" => -1,
        "con" => 2,
        "task" => 3,
        "irq" => 4,
        _ => return Err(format!("tipo de capacidade desconhecido '{kind}'")),
    };
    Ok(CapSpec {
        slot: slot.parse().map_err(|_| bad())?,
        kind,
        obj: obj.parse().map_err(|_| bad())?,
        badge: badge.parse().map_err(|_| bad())?,
    })
}

/// Lê o manifesto, monta tudo e constrói a imagem.
pub fn load_manifest(path: &Path) -> Result<SystemImage, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut kernel = None;
    let mut tasks = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split(['#', ';']).next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let at = |m: String| format!("{}:{}: {m}", path.display(), n + 1);
        match f[0] {
            "kernel" if f.len() == 2 => {
                kernel = Some(assemble_file(&dir.join(f[1])).map_err(|e| at(e.to_string()))?);
            }
            "task" if f.len() >= 3 => {
                let program = assemble_file(&dir.join(f[2])).map_err(|e| at(e.to_string()))?;
                let mut spec = TaskSpec::new(f[1], program);
                let num = |s: &str| s.parse::<i64>().map_err(|_| at(format!("número inválido '{s}'")));
                for c in &f[3..] {
                    if let Some(p) = c.strip_prefix("stack=") {
                        spec.stack_pages = num(p)?;
                    } else if let Some(p) = c.strip_prefix("fault=") {
                        let (e, b) = p.split_once('/').unwrap_or((p, "0"));
                        spec.fault = Some((num(e)?, num(b)?));
                    } else if let Some(p) = c.strip_prefix("irqline=") {
                        let (l, rest) = p.split_once(':').ok_or_else(|| at(format!("irqline inválido '{c}'")))?;
                        let (e, b) = rest.split_once('/').unwrap_or((rest, "0"));
                        spec.irq = Some((num(l)?, num(e)?, num(b)?));
                    } else if let Some(p) = c.strip_prefix("dev=") {
                        let (d, v) = p.split_once('@').ok_or_else(|| at(format!("dev inválido '{c}'")))?;
                        let phys = match d {
                            "console" => crate::machine::DEV_CONSOLE,
                            _ => return Err(at(format!("dispositivo desconhecido '{d}'"))),
                        };
                        spec.device = Some((phys, num(v)?));
                    } else {
                        spec.caps.push(parse_cap(c).map_err(at)?);
                    }
                }
                tasks.push(spec);
            }
            _ => return Err(at(format!("linha não reconhecida: {line}"))),
        }
    }
    let kernel = kernel.ok_or_else(|| format!("{}: falta a linha 'kernel'", path.display()))?;
    build_system(&kernel, &tasks)
}
