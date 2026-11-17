//! TENERURA — construtor da imagem de sistema: núcleo + tarefas iniciais.
//!
//! Faz o papel do "firmware de carga": coloca o núcleo nos endereços
//! físicos dele, empilha os módulos (texto e dados de cada tarefa) depois
//! do núcleo e preenche a TABELA DE BOOT que o núcleo lê ao iniciar. Os
//! deslocamentos da tabela vêm dos símbolos do próprio núcleo — uma única
//! fonte de verdade (src/os/kernel/kernel.tas).

use std::collections::BTreeMap;

use crate::asm::{page_start_at_or_after, Program};
use crate::machine::{page_of, Machine, PAGE_HALF, PAGE_SIZE, VA_MAX};
use crate::word::Word;

/// Capacidade inicial de uma tarefa.
#[derive(Clone, Debug)]
pub struct CapSpec {
    pub slot: usize,
    pub kind: i64,
    pub obj: i64,
    pub badge: i64,
}

#[derive(Clone, Debug)]
pub struct TaskSpec {
    pub name: String,
    pub program: Program,
    pub stack_pages: i64,
    pub caps: Vec<CapSpec>,
    /// aviso de morte: (endpoint, selo) — quem recebe nesse endpoint supervisiona
    pub fault: Option<(i64, i64)>,
    /// interrupção atendida: (linha, endpoint onde chega como mensagem, selo)
    pub irq: Option<(i64, i64, i64)>,
    /// página de dispositivo concedida: (centro físico, página virtual)
    pub device: Option<(i64, i64)>,
}

impl TaskSpec {
    pub fn new(name: &str, program: Program) -> TaskSpec {
        TaskSpec { name: name.to_string(), program, stack_pages: 1, caps: vec![], fault: None, irq: None, device: None }
    }
    pub fn cap(mut self, slot: usize, kind: i64, obj: i64, badge: i64) -> TaskSpec {
        self.caps.push(CapSpec { slot, kind, obj, badge });
        self
    }
    pub fn fault(mut self, ep: i64, badge: i64) -> TaskSpec {
        self.fault = Some((ep, badge));
        self
    }
    pub fn irq(mut self, line: i64, ep: i64, badge: i64) -> TaskSpec {
        self.irq = Some((line, ep, badge));
        self
    }
    pub fn device(mut self, phys: i64, vpn: i64) -> TaskSpec {
        self.device = Some((phys, vpn));
        self
    }
    pub fn stack(mut self, pages: i64) -> TaskSpec {
        self.stack_pages = pages;
        self
    }
}

/// Tipos de capacidade (espelham src/os/lib/abi.inc).
pub const CAP_EP_SEND: i64 = 1;
pub const CAP_EP_RECV: i64 = -1;
pub const CAP_CONSOLE: i64 = 2;
pub const CAP_TASK: i64 = 3;
pub const CAP_IRQ: i64 = 4;

#[derive(Clone, Debug)]
pub struct SystemImage {
    /// blocos (endereço físico base, palavras)
    pub blocks: Vec<(i64, Vec<Word>)>,
    pub kernel_symbols: BTreeMap<String, i64>,
    pub task_names: Vec<String>,
    /// por tarefa: página de dispositivo concedida (centro físico)
    pub task_devices: Vec<Option<i64>>,
}

impl SystemImage {
    /// Carrega a imagem na RAM. A máquina deve estar em estado de reset.
    pub fn load(&self, m: &mut Machine) -> Result<(), String> {
        self.blocks.iter().try_for_each(|(base, words)| m.load(*base, words))
    }

    pub fn ksym(&self, name: &str) -> i64 {
        *self
            .kernel_symbols
            .get(name)
            .unwrap_or_else(|| panic!("símbolo do núcleo ausente: {name}"))
    }
}

pub fn build_system(kernel: &Program, tasks: &[TaskSpec]) -> Result<SystemImage, String> {
    build_system_with(kernel, tasks, 0)
}

/// Como `build_system`, com a fatia de tempo do escalonador (0 = padrão do
/// núcleo). Variar a fatia muda o entrelaçamento das tarefas.
pub fn build_system_with(kernel: &Program, tasks: &[TaskSpec], quantum: i64) -> Result<SystemImage, String> {
    let k = |name: &str| -> Result<i64, String> {
        kernel
            .symbols
            .get(name)
            .copied()
            .ok_or_else(|| format!("o núcleo não define o símbolo {name}"))
    };
    if kernel.entry != 0 {
        return Err(format!("o núcleo deve começar no vetor de reset 0 (entrada = {})", kernel.entry));
    }
    let (boot_table, magic, bt_tasks, be_size, max_tasks) =
        (k("boot_table")?, k("BOOT_MAGIC")?, k("BT_TASKS")?, k("BE_SIZE")?, k("MAX_TASKS")?);
    let (be_caps, cap_slots) = (k("BE_CAPS")?, k("CAP_SLOTS")?);
    if tasks.is_empty() || tasks.len() as i64 > max_tasks {
        return Err(format!("número de tarefas inválido: {} (máximo {max_tasks})", tasks.len()));
    }

    let mut blocks: Vec<(i64, Vec<Word>)> =
        kernel.segments.iter().map(|s| (s.base, s.words.clone())).collect();
    let kernel_top = kernel.segments.iter().map(|s| s.end()).max().unwrap();
    let mut cursor = page_start_at_or_after(kernel_top);

    // tabela de boot montada à parte e depois escrita sobre o segmento de dados
    let mut table = vec![Word::ZERO; (bt_tasks + be_size * max_tasks) as usize];
    table[k("BT_MAGIC")? as usize] = Word::lit(magic);
    table[k("BT_NTASKS")? as usize] = Word::lit(tasks.len() as i64);
    table[k("BT_QUANTUM")? as usize] = Word::lit(quantum);

    for (i, t) in tasks.iter().enumerate() {
        let text = t.program.text();
        let e = (bt_tasks + be_size * i as i64) as usize;
        let mut put = |off: i64, v: i64| table[e + off as usize] = Word::lit(v);
        if text.words.is_empty() {
            return Err(format!("tarefa {}: texto vazio", t.name));
        }
        for s in &t.program.segments {
            if s.base.abs() > VA_MAX || s.end().abs() > VA_MAX {
                return Err(format!("tarefa {}: segmento fora do espaço virtual de 18 trits", t.name));
            }
            if page_of(s.base) <= 0 {
                return Err(format!("tarefa {}: segmento em página ≤ 0 (reservadas à guarda e à pilha)", t.name));
            }
        }
        if t.program.entry < text.base || t.program.entry >= text.end() {
            return Err(format!("tarefa {}: entrada fora do texto", t.name));
        }
        put(k("BE_TEXT_PHYS")?, cursor);
        put(k("BE_TEXT_LEN")?, text.words.len() as i64);
        put(k("BE_TEXT_VBASE")?, text.base);
        blocks.push((cursor, text.words.clone()));
        cursor += text.words.len() as i64;
        if let Some(data) = t.program.data() {
            if page_of(data.base) <= page_of(text.end() - 1) {
                return Err(format!("tarefa {}: dados e texto na mesma página", t.name));
            }
            put(k("BE_DATA_PHYS")?, cursor);
            put(k("BE_DATA_LEN")?, data.words.len() as i64);
            put(k("BE_DATA_VBASE")?, data.base);
            blocks.push((cursor, data.words.clone()));
            cursor += data.words.len() as i64;
        }
        put(k("BE_ENTRY")?, t.program.entry);
        put(k("BE_STACK_PAGES")?, t.stack_pages);
        if t.stack_pages < 1 {
            return Err(format!("tarefa {}: precisa de ao menos uma página de pilha", t.name));
        }
        if let Some((ep, badge)) = t.fault {
            if ep == 0 {
                return Err(format!("tarefa {}: endpoint de aviso 0 significa 'nenhum'", t.name));
            }
            if badge >= 0 {
                return Err(format!("tarefa {}: selo de aviso de morte deve ser negativo (origem: núcleo)", t.name));
            }
            put(k("BE_FAULT_EP")?, ep);
            put(k("BE_FAULT_BADGE")?, badge);
        }
        if let Some((line, ep, badge)) = t.irq {
            if !(2..=27).contains(&line) {
                return Err(format!("tarefa {}: linha de interrupção {line} fora de 2..27", t.name));
            }
            if tasks[..i].iter().any(|o| o.irq.map(|x| x.0) == Some(line)) {
                return Err(format!("tarefa {}: linha {line} já tem driver", t.name));
            }
            if badge >= 0 {
                return Err(format!("tarefa {}: selo de interrupção deve ser negativo (origem: núcleo)", t.name));
            }
            put(k("BE_IRQ_LINE")?, line);
            put(k("BE_IRQ_EP")?, ep);
            put(k("BE_IRQ_BADGE")?, badge);
        }
        if let Some((phys, vpn)) = t.device {
            if phys != crate::machine::DEV_CONSOLE {
                return Err(format!("tarefa {}: {phys} não é uma página de dispositivo concedível", t.name));
            }
            if tasks[..i].iter().any(|o| o.device.map(|x| x.0) == Some(phys)) {
                return Err(format!("tarefa {}: dispositivo {phys} já concedido a outra tarefa", t.name));
            }
            let used = t.program.segments.iter().any(|s| (page_of(s.base)..=page_of(s.end() - 1)).contains(&vpn));
            if vpn <= 0 || used || (vpn * PAGE_SIZE).abs() + PAGE_HALF > VA_MAX {
                return Err(format!("tarefa {}: página virtual {vpn} inválida para o dispositivo", t.name));
            }
            put(k("BE_DEV_PHYS")?, phys);
            put(k("BE_DEV_VPN")?, vpn);
        }
        for c in &t.caps {
            if c.slot as i64 >= cap_slots {
                return Err(format!("tarefa {}: slot de capacidade {} inexistente", t.name, c.slot));
            }
            // Selos NEGATIVOS são reservados às mensagens do núcleo (avisos de
            // morte, interrupções): quem recebe distingue a origem pelo sinal.
            if c.kind == CAP_EP_SEND && c.badge < 0 {
                return Err(format!("tarefa {}: selo negativo em capacidade de envio (reservado ao núcleo)", t.name));
            }
            if c.kind == CAP_TASK && !(0..tasks.len() as i64).contains(&c.obj) {
                return Err(format!("tarefa {}: capacidade para tarefa inexistente {}", t.name, c.obj));
            }
            if c.kind == CAP_IRQ && !(2..=27).contains(&c.obj) {
                return Err(format!("tarefa {}: capacidade para linha de interrupção inválida {}", t.name, c.obj));
            }
            let base = be_caps + 3 * c.slot as i64;
            put(base, c.kind);
            put(base + 1, c.obj);
            put(base + 2, c.badge);
        }
    }
    // primeiro quadro inteiramente livre depois dos módulos
    let first_free_page = (cursor + PAGE_HALF + PAGE_SIZE - 1).div_euclid(PAGE_SIZE);
    table[k("BT_FREE")? as usize] = Word::lit(first_free_page * PAGE_SIZE);

    // escreve a tabela sobre o segmento do núcleo que a contém
    let mut patched = false;
    for (base, words) in blocks.iter_mut() {
        let end = *base + words.len() as i64;
        if boot_table >= *base && boot_table + table.len() as i64 <= end {
            let off = (boot_table - *base) as usize;
            words[off..off + table.len()].copy_from_slice(&table);
            patched = true;
            break;
        }
    }
    if !patched {
        return Err("boot_table não cabe em nenhum segmento do núcleo".into());
    }

    Ok(SystemImage {
        blocks,
        kernel_symbols: kernel.symbols.clone(),
        task_names: tasks.iter().map(|t| t.name.clone()).collect(),
        task_devices: tasks.iter().map(|t| t.device.map(|d| d.0)).collect(),
    })
}

// ───────────────────────── inspeção do núcleo pelo hospedeiro ─────────────────────────

/// Estado de uma tarefa lido direto da RAM (como um depurador externo).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskView {
    pub id: usize,
    pub name: String,
    pub state: i64,
    pub pc: i64,
    pub regs: [i64; 27],
    pub ep: i64,
    pub seq: i64,
    pub peer: i64,
    pub timed: i64,
    pub deadline: i64,
    pub fcause: i64,
    pub fval: i64,
    pub sendkind: i64,
    pub exit: i64,
    pub ptroot: i64,
    pub irq: i64,
    pub irqpend: i64,
    pub gen: i64,
    /// endereço do centro do TCB
    pub tcb: i64,
}

impl TaskView {
    pub fn reg(&self, r: i8) -> i64 {
        self.regs[(r + 13) as usize]
    }
}

/// Lê a tabela de tarefas do núcleo. Só leitura; não altera a máquina.
pub fn task_table(img: &SystemImage, m: &Machine) -> Vec<TaskView> {
    let k = |n: &str| img.ksym(n);
    let rd = |a: i64| m.peek(a).expect("TCB fora da RAM").val();
    let (tcbs, size) = (k("tcbs"), k("TCB_SIZE"));
    (0..img.task_names.len())
        .map(|i| {
            let c = tcbs + 40 + size * i as i64;
            let mut regs = [0i64; 27];
            for r in -13..=13i64 {
                regs[(r + 13) as usize] = if r == 0 { 0 } else { rd(c + r) };
            }
            TaskView {
                id: i,
                name: img.task_names[i].clone(),
                state: rd(c + k("T_STATE")),
                pc: rd(c + k("T_PC")),
                regs,
                ep: rd(c + k("T_EP")),
                seq: rd(c + k("T_SEQ")),
                peer: rd(c + k("T_PEER")),
                timed: rd(c + k("T_TIMED")),
                deadline: rd(c + k("T_DEADLINE")),
                fcause: rd(c + k("T_FCAUSE")),
                fval: rd(c + k("T_FVAL")),
                sendkind: rd(c + k("T_SENDKIND")),
                exit: rd(c + k("T_EXIT")),
                ptroot: rd(c + k("T_PTROOT")),
                irq: rd(c + k("T_IRQ")),
                irqpend: rd(c + k("T_IRQPEND")),
                gen: rd(c + k("T_GEN")),
                tcb: c,
            }
        })
        .collect()
}

/// Nome legível do estado de uma tarefa.
pub fn state_name(img: &SystemImage, s: i64) -> &'static str {
    let k = |n: &str| img.ksym(n);
    if s == k("ST_READY") {
        "pronta"
    } else if s == k("ST_FREE") {
        "livre"
    } else if s == k("ST_BLK_SEND") {
        "bloq-envio"
    } else if s == k("ST_BLK_RECV") {
        "bloq-recepção"
    } else if s == k("ST_BLK_REPLY") {
        "bloq-resposta"
    } else if s == k("ST_SLEEP") {
        "dormindo"
    } else if s == k("ST_ZOMBIE") {
        "zumbi"
    } else if s == k("ST_DEAD") {
        "morta"
    } else {
        "INVÁLIDO"
    }
}
