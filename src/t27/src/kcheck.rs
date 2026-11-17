//! TENERURA — verificador de invariantes do micronúcleo, do lado do
//! hospedeiro (verificação em tempo de execução, como um depurador externo
//! que inspeciona a RAM). Os testes chamam `check_state` a CADA saída do
//! núcleo (ERET) e `check_memory` periodicamente.
//!
//! O que é verificado está enumerado em docs/08-micronucleo.md §6 (I1–I12).
//! Isto é TESTE de invariantes sobre execuções concretas, não prova.

use std::collections::HashMap;

use crate::machine::{Machine, PAGE_HALF, PAGE_SIZE, PAGE_TRITS, PTE_TABLE};
use crate::sysimage::{task_table, SystemImage, TaskView};
use crate::word::Word;

/// Profundidade máxima admitida para a pilha do núcleo (palavras).
pub const KSTACK_LIMIT: i64 = 64;

pub struct KCheck {
    /// cópia do texto do núcleo e dos módulos de boot (integridade)
    frozen: Vec<(i64, Vec<Word>)>,
    /// cópia da zona que a pilha do núcleo NÃO pode alcançar
    below_stack: Vec<Word>,
    /// centro do primeiro quadro que o núcleo pode entregar a tarefas
    free_start: i64,
    ntasks: usize,
    /// maior profundidade de pilha do núcleo observada
    pub max_kstack: i64,
}

/// Mapeamento de uma página de usuário.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leaf {
    pub vpn: i64,
    pub frame: i64,
    pub w: bool,
    pub x: bool,
}

/// Percorre as tabelas de uma tarefa. Devolve (quadros de tabela, folhas).
pub fn walk_tables(m: &Machine, root: i64) -> Result<(Vec<i64>, Vec<Leaf>), String> {
    let rd = |a: i64| m.peek(a).ok_or_else(|| format!("tabela fora da RAM em {a}"));
    if root % PAGE_SIZE != 0 {
        return Err(format!("raiz {root} não é centro de quadro"));
    }
    let mut tables = vec![root];
    let mut leaves = Vec::new();
    for i1 in -PAGE_HALF..=PAGE_HALF {
        let e = rd(root + i1)?;
        if e.val() == 0 {
            continue;
        }
        if e.field(0, PAGE_TRITS) != PTE_TABLE {
            return Err(format!("raiz[{i1}] = {e} não é ponteiro de tabela"));
        }
        let t = e.val() - PTE_TABLE;
        tables.push(t);
        for i2 in -PAGE_HALF..=PAGE_HALF {
            let p = rd(t + i2)?;
            if p.val() == 0 {
                continue;
            }
            let (v, w, x) = (p.field(0, 1), p.field(1, 1), p.field(2, 1));
            if v != 1 || w < 0 || x < 0 || p.field(3, 3) != 0 {
                return Err(format!("PTE inválida {p} em [{i1}][{i2}]"));
            }
            leaves.push(Leaf {
                vpn: i1 * PAGE_SIZE + i2,
                frame: p.val() - p.field(0, PAGE_TRITS),
                w: w == 1,
                x: x == 1,
            });
        }
    }
    Ok((tables, leaves))
}

impl KCheck {
    /// Tirar logo após carregar a imagem, antes de executar.
    pub fn new(img: &SystemImage, m: &Machine) -> KCheck {
        let (t0, t1) = (img.ksym("__text_start"), img.ksym("__text_end"));
        let mut frozen = vec![(t0, (t0..t1).map(|a| m.peek(a).unwrap()).collect())];
        // módulos de boot: os blocos depois dos dois segmentos do núcleo
        for (base, words) in img.blocks.iter().skip(2) {
            frozen.push((*base, words.clone()));
        }
        let free_start = m.peek(img.ksym("boot_table") + img.ksym("BT_FREE")).unwrap().val();
        KCheck {
            frozen,
            below_stack: (-PAGE_HALF..-KSTACK_LIMIT).map(|a| m.peek(a).unwrap()).collect(),
            free_start,
            ntasks: img.task_names.len(),
            max_kstack: 0,
        }
    }

    /// Invariantes de estado (baratos). `at_eret`: a máquina acabou de sair
    /// do núcleo para a tarefa corrente.
    pub fn check_state(&mut self, img: &SystemImage, m: &Machine, at_eret: bool) -> Result<(), String> {
        let k = |n: &str| img.ksym(n);
        let rd = |a: i64| m.peek(a).map(|w| w.val()).ok_or_else(|| format!("leitura fora da RAM em {a}"));
        let tasks = task_table(img, m);
        let by_tcb: HashMap<i64, &TaskView> = tasks.iter().map(|t| (t.tcb, t)).collect();
        let (ready, free, bsend, brecv, breply, sleep, zombie, dead) = (
            k("ST_READY"),
            k("ST_FREE"),
            k("ST_BLK_SEND"),
            k("ST_BLK_RECV"),
            k("ST_BLK_REPLY"),
            k("ST_SLEEP"),
            k("ST_ZOMBIE"),
            k("ST_DEAD"),
        );
        let alive = |s: i64| s != dead && s != zombie && s != free;

        // I1: estados válidos; slots além das tarefas existentes ficam livres
        for t in &tasks {
            if ![ready, bsend, brecv, breply, sleep, zombie, dead].contains(&t.state) {
                return Err(format!("I1: tarefa {} com estado inválido {}", t.name, t.state));
            }
        }
        let (tcbs, size, max) = (k("tcbs"), k("TCB_SIZE"), k("MAX_TASKS"));
        for i in self.ntasks as i64..max {
            if rd(tcbs + 40 + size * i + k("T_STATE"))? != free {
                return Err(format!("I1: slot {i} deveria estar livre"));
            }
        }

        // I2: na saída do núcleo, a máquina está configurada para a corrente
        if at_eret {
            let cur = rd(k("cur"))?;
            let Some(t) = by_tcb.get(&cur) else {
                return Err(format!("I2: cur = {cur} não é um TCB"));
            };
            if t.state != ready {
                return Err(format!("I2: tarefa corrente {} não está pronta (estado {})", t.name, t.state));
            }
            if m.kernel || !m.ie {
                return Err("I2: saída do núcleo sem modo usuário + interrupções".into());
            }
            if m.ptroot.val() != t.ptroot || m.scratch.val() != cur || m.pc.val() != t.pc {
                return Err(format!("I2: PTROOT/SCRATCH/PC não correspondem à tarefa {}", t.name));
            }
            for r in -13..=13i8 {
                if m.reg(r).val() != t.reg(r) {
                    return Err(format!("I2: registrador {r} restaurado errado para {}", t.name));
                }
            }
            if !by_tcb.contains_key(&rd(k("last"))?) {
                return Err("I2: last não é um TCB".into());
            }
        }

        for t in &tasks {
            // I3: quem aguarda resposta aguarda uma tarefa VIVA; ninguém mais tem peer
            if t.state == breply {
                let Some(s) = by_tcb.get(&t.peer) else {
                    return Err(format!("I3: {} aguarda resposta de TCB inválido {}", t.name, t.peer));
                };
                if !alive(s.state) {
                    return Err(format!("I3: {} aguarda resposta de {}, que morreu", t.name, s.name));
                }
            } else if t.peer != 0 {
                return Err(format!("I3: {} tem peer sem estar aguardando resposta", t.name));
            }
            // I4: prazo só em quem espera; todo sleep tem prazo
            let waiting = [sleep, bsend, brecv, breply].contains(&t.state);
            if t.timed != 0 && !waiting {
                return Err(format!("I4: {} tem prazo armado sem estar esperando (estado {})", t.name, t.state));
            }
            if t.state == sleep && t.timed == 0 {
                return Err(format!("I4: {} dorme sem prazo", t.name));
            }
            // I10: interrupção pendente só em driver vivo
            if t.irqpend != 0 && (t.irq == 0 || !alive(t.state)) {
                return Err(format!("I10: {} com interrupção pendente sem ser driver vivo", t.name));
            }
            // I12: toda capacidade não vazia é bem formada — tipo conhecido,
            // objeto dentro do domínio do tipo. Slot vazio é zero puro.
            // Desde cap_grant as capacidades mudam de dono em execução: mover
            // autoridade não pode inventar direito novo.
            for i in 0..k("CAP_SLOTS") {
                let base = t.tcb + k("T_CAPS") + 3 * i;
                let (kind, obj, badge) =
                    (rd(base + k("CAP_KIND"))?, rd(base + k("CAP_OBJ"))?, rd(base + k("CAP_BADGE"))?);
                let bad = |why: String| format!("I12: {} slot {i} {why}", t.name);
                match kind {
                    0 if obj != 0 || badge != 0 => return Err(bad(format!("vazio com resto sujo ({obj},{badge})"))),
                    0 | -1 => {}
                    1 if badge < 0 => return Err(bad("de envio com selo negativo".into())),
                    1 => {}
                    2 if obj != 0 => return Err(bad(format!("de console com objeto {obj} (não usado)"))),
                    2 => {}
                    3 => {
                        if !(0..self.ntasks as i64).contains(&obj) {
                            return Err(bad(format!("para tarefa inexistente {obj}")));
                        }
                        if rd(tcbs + 40 + size * obj + k("T_STATE"))? == free {
                            return Err(bad(format!("para slot livre {obj}")));
                        }
                    }
                    4 if !(2..=27).contains(&obj) => return Err(bad(format!("para linha inválida {obj}"))),
                    4 => {}
                    _ => return Err(bad(format!("com tipo desconhecido {kind}"))),
                }
            }
        }

        // I5: nenhum endpoint com emissor (ou aviso de morte) E receptor esperando
        // I6: carimbos únicos entre todos os que esperam
        let mut seqs = HashMap::new();
        for t in tasks.iter().filter(|t| [bsend, brecv, breply, zombie].contains(&t.state)) {
            if let Some(o) = seqs.insert(t.seq, &t.name) {
                return Err(format!("I6: {} e {} com o mesmo carimbo {}", o, t.name, t.seq));
            }
            if t.seq == 0 {
                return Err(format!("I6: {} espera com carimbo zero", t.name));
            }
            if t.state == bsend || t.state == zombie {
                if let Some(r) = tasks.iter().find(|r| r.state == brecv && r.ep == t.ep) {
                    return Err(format!("I5: {} envia e {} recebe no endpoint {} sem se encontrarem", t.name, r.name, t.ep));
                }
            }
            if t.state == bsend && t.sendkind != 1 && t.sendkind != 2 {
                return Err(format!("I5: {} bloqueada em envio com tipo {}", t.name, t.sendkind));
            }
        }

        // I11: a máscara de interrupções da máquina é a cópia do núcleo, e as
        // linhas habilitadas são o timer mais, no máximo, as de drivers vivos
        if at_eret {
            let shadow = rd(k("imask_shadow"))?;
            let mut want = 0i64;
            for line in 0..27 {
                if m.imask >> line & 1 == 1 {
                    want += crate::word::POW3[line as usize];
                }
            }
            if shadow != want {
                return Err(format!("I11: máscara da máquina {want} ≠ cópia do núcleo {shadow}"));
            }
            if m.imask & 1 == 0 {
                return Err("I11: timer mascarado com tarefa rodando".into());
            }
            for line in 2..=27i64 {
                if m.imask >> (line - 1) & 1 == 1 && !tasks.iter().any(|t| t.irq == line && alive(t.state)) {
                    return Err(format!("I11: linha {line} habilitada sem driver vivo"));
                }
            }
        }

        // I9: a pilha do núcleo nunca passou do limite
        for (i, a) in (-PAGE_HALF..-KSTACK_LIMIT).enumerate() {
            if m.peek(a).unwrap() != self.below_stack[i] {
                return Err(format!("I9: pilha do núcleo alcançou {a}"));
            }
        }
        Ok(())
    }

    /// Mede a profundidade da pilha do núcleo usada até agora (precisa que a
    /// zona da pilha tenha começado zerada).
    pub fn measure_kstack(&mut self, m: &Machine) {
        if self.below_stack.iter().any(|w| w.val() != 0) {
            return; // RAM iniciada com lixo: a medida não vale
        }
        for d in (1..=KSTACK_LIMIT).rev() {
            if m.peek(-d).unwrap().val() != 0 {
                self.max_kstack = self.max_kstack.max(d);
                break;
            }
        }
    }

    /// Invariantes de memória (mais caros): integridade do núcleo e
    /// isolamento estrutural dos espaços de endereços.
    pub fn check_memory(&self, img: &SystemImage, m: &Machine) -> Result<(), String> {
        // I7: texto do núcleo e módulos de boot intactos
        for (base, words) in &self.frozen {
            for (i, w) in words.iter().enumerate() {
                if m.peek(base + i as i64) != Some(*w) {
                    return Err(format!("I7: memória protegida alterada em {}", base + i as i64));
                }
            }
        }
        // I8: nenhum quadro alcançável por duas tarefas; nenhuma tabela, raiz
        // ou quadro do núcleo mapeado em espaço de usuário; W e X nunca juntos;
        // página 0 nunca mapeada.
        let tasks = task_table(img, m);
        let mut owner: HashMap<i64, String> = HashMap::new();
        let mut all_tables: HashMap<i64, String> = HashMap::new();
        let mut all_leaves = Vec::new();
        for t in tasks.iter().filter(|t| t.state != img.ksym("ST_FREE")) {
            let (tables, leaves) = walk_tables(m, t.ptroot).map_err(|e| format!("I8: tarefa {}: {e}", t.name))?;
            for tb in tables {
                if tb < self.free_start {
                    return Err(format!("I8: tabela de {} em quadro do núcleo ({tb})", t.name));
                }
                if let Some(o) = all_tables.insert(tb, t.name.clone()) {
                    return Err(format!("I8: tabela {tb} compartilhada por {o} e {}", t.name));
                }
            }
            for l in leaves {
                if l.w && l.x {
                    return Err(format!("I8: página {} de {} gravável e executável", l.vpn, t.name));
                }
                if l.vpn == 0 {
                    return Err(format!("I8: página 0 (guarda de nulo) mapeada em {}", t.name));
                }
                if img.task_devices[t.id] == Some(l.frame) {
                    // página de dispositivo concedida a ESTA tarefa: gravável, nunca executável
                    if l.x {
                        return Err(format!("I8: dispositivo executável em {}", t.name));
                    }
                } else if l.frame < self.free_start || m.peek(l.frame + PAGE_HALF).is_none() {
                    return Err(format!("I8: {} mapeia quadro {} fora da área de usuário", t.name, l.frame));
                }
                if let Some(o) = owner.insert(l.frame, t.name.clone()) {
                    return Err(format!("I8: quadro {} alcançável por {o} e {}", l.frame, t.name));
                }
                all_leaves.push((t.name.clone(), l));
            }
        }
        for (name, l) in &all_leaves {
            if let Some(o) = all_tables.get(&l.frame) {
                return Err(format!("I8: {name} mapeia como dados uma tabela de páginas de {o}"));
            }
        }
        Ok(())
    }
}
