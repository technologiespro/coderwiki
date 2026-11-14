// Ядро формата .xdp (XDP2): ZSTD + генетически обученный словарь.
// Используется и CLI-утилитой (main.rs, для Node-backend), и Tauri desktop app.
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAGIC: &[u8; 4] = b"XDP2";
const EVAL_LEVEL: i32 = 12; // быстрый уровень для оценки фитнеса
const EVAL_CAP: usize = 8 * 1024 * 1024;
const DICT_SIZES: [usize; 4] = [4096, 8192, 16384, 32768];

// Простой xorshift64 — без внешних зависимостей
struct Rng(u64);
impl Rng {
    fn new() -> Self {
        Rng((SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn chance(&mut self, p: f64) -> bool {
        (self.next() % 10000) as f64 / 10000.0 < p
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[derive(Clone)]
struct Genome {
    mask: Vec<bool>,
    dict_size: usize,
    fit: Option<usize>, // кэш фитнеса — элита не переобучается каждое поколение
}

fn train(samples: &[Vec<u8>], g: &Genome) -> Option<Vec<u8>> {
    let sel: Vec<&[u8]> = samples.iter().zip(&g.mask).filter(|(_, on)| **on).map(|(s, _)| s.as_slice()).collect();
    if sel.len() < 8 {
        return None;
    }
    zstd::dict::from_samples(&sel, g.dict_size).ok()
}

// FNV-1a для дедупликации одинаковых сэмплов (копии файлов, шаблоны)
fn fnv(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3))
}

// Доля повторяющихся 4-граммных токенов: сэмплы с высокой повторяемостью полезнее для словаря
fn redundancy(data: &[u8]) -> f64 {
    if data.len() < 8 {
        return 0.0;
    }
    let mut seen = std::collections::HashSet::with_capacity(data.len() / 2);
    let mut dup = 0usize;
    for w in data.windows(4).step_by(2) {
        if !seen.insert(u32::from_le_bytes([w[0], w[1], w[2], w[3]])) {
            dup += 1;
        }
    }
    dup as f64 / (data.len() / 2).max(1) as f64
}

// Фитнес — реальная цель: размер сжатого tar со словарём + размер словаря
fn fitness(whole: &[u8], dict: &[u8]) -> usize {
    match zstd::bulk::Compressor::with_dictionary(EVAL_LEVEL, dict) {
        Ok(mut c) => match c.compress(whole) {
            Ok(v) => v.len() + dict.len(),
            Err(_) => usize::MAX,
        },
        Err(_) => usize::MAX,
    }
}

// Генетический подбор словаря с бюджетом времени; on_line — прогресс по поколениям.
// Особи — маска сэмплов + размер словаря; фитнес — реальный размер сжатого tar + словарь.
pub fn evolve_dictionary<F: FnMut(String)>(
    raw_samples: &[Vec<u8>],
    whole_tar: &[u8],
    opts: &PackOptions,
    mut on_line: F,
) -> Vec<u8> {
    // Дедупликация одинаковых сэмплов — иначе копии перевешивают словарь
    let mut seen = std::collections::HashSet::new();
    let samples: Vec<&Vec<u8>> = raw_samples.iter().filter(|s| seen.insert(fnv(s))).collect();
    let samples: Vec<Vec<u8>> = samples.into_iter().cloned().collect();
    if samples.len() < 8 {
        return Vec::new();
    }
    let max_dict = opts.dict_size;
    let gens = opts.gens.max(1);
    let pop_size = opts.pop.max(2);
    let budget = std::time::Duration::from_millis(opts.time_budget_ms);
    let started = std::time::Instant::now();
    let whole = &whole_tar[..whole_tar.len().min(EVAL_CAP)];
    on_line(format!(
        "training: {} сэмплов ({} уникальных), до {} поколений, бюджет {} с",
        raw_samples.len(),
        samples.len(),
        gens,
        budget.as_secs()
    ));
    let mut rng = Rng::new();
    let n = samples.len();
    // Стартовая популяция: все сэмплы, «избыточные» по частоте токенов, случайные маски
    let red: Vec<f64> = samples.iter().map(|s| redundancy(s)).collect();
    let mut sorted_red = red.clone();
    sorted_red.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted_red[n / 2];
    let mut population: Vec<Genome> = vec![
        Genome { mask: vec![true; n], dict_size: max_dict.min(16384), fit: None },
        Genome { mask: red.iter().map(|r| *r >= median).collect(), dict_size: max_dict.min(16384), fit: None },
    ];
    while population.len() < pop_size {
        let mask: Vec<bool> = (0..n).map(|_| rng.chance(0.7)).collect();
        population.push(Genome { mask, dict_size: DICT_SIZES[rng.range(DICT_SIZES.len())].min(max_dict), fit: None });
    }
    let mut best_dict = Vec::new();
    let mut best_fit = usize::MAX;
    let mut stale = 0usize;
    for gen in 0..gens {
        let prev_best = best_fit;
        for g in population.iter_mut() {
            if g.fit.is_some() {
                continue;
            }
            g.fit = Some(match train(&samples, g) {
                Some(d) => {
                    let f = fitness(whole, &d);
                    if f < best_fit {
                        best_fit = f;
                        best_dict = d;
                    }
                    f
                }
                None => usize::MAX,
            });
            if started.elapsed() > budget {
                break;
            }
        }
        population.sort_by_key(|g| g.fit.unwrap_or(usize::MAX));
        on_line(format!("gen {}/{}: лучший фитнес {} ({:.1} с)", gen + 1, gens, best_fit, started.elapsed().as_secs_f64()));
        stale = if best_fit < prev_best { 0 } else { stale + 1 };
        if started.elapsed() > budget {
            on_line("бюджет времени исчерпан — берём лучший найденный словарь".to_string());
            break;
        }
        if stale >= 6 {
            on_line("фитнес не улучшается 6 поколений — ранняя остановка".to_string());
            break;
        }
        // Элита + потомки (турнир, скрещивание, мутация)
        let scored = population;
        let mut next: Vec<Genome> = scored.iter().take(2).cloned().collect();
        while next.len() < pop_size {
            let a = &scored[rng.range(scored.len().min(3))];
            let b = &scored[rng.range(scored.len().min(4))];
            let mut child = Genome {
                mask: a.mask.iter().zip(&b.mask).map(|(x, y)| if rng.chance(0.5) { *x } else { *y }).collect(),
                dict_size: if rng.chance(0.5) { a.dict_size } else { b.dict_size },
                fit: None,
            };
            for bit in child.mask.iter_mut() {
                if rng.chance(0.08) {
                    *bit = !*bit;
                }
            }
            if rng.chance(0.2) {
                child.dict_size = DICT_SIZES[rng.range(DICT_SIZES.len())].min(max_dict);
            }
            next.push(child);
        }
        population = next;
    }
    // Словарь оправдан только при реальном выигрыше
    let base = zstd::bulk::compress(whole, EVAL_LEVEL).map(|v| v.len()).unwrap_or(usize::MAX);
    if best_fit < base {
        on_line(format!("словарь принят: выигрыш {} байт", base - best_fit));
        best_dict
    } else {
        on_line("словарь отклонён — сжимаем без словаря".to_string());
        Vec::new()
    }
}

// Извлечение содержимого файлов из tar как обучающих сэмплов
pub fn tar_samples(tar_bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut samples = Vec::new();
    let mut archive = tar::Archive::new(tar_bytes);
    if let Ok(entries) = archive.entries() {
        for entry in entries.flatten() {
            let mut e = entry;
            let mut buf = Vec::new();
            if e.read_to_end(&mut buf).is_ok() && buf.len() >= 64 {
                buf.truncate(65536);
                samples.push(buf);
            }
        }
    }
    samples
}

pub struct PackOptions {
    pub level: i32,
    pub dict_size: usize,
    pub gens: usize,
    pub pop: usize,
    pub time_budget_ms: u64,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self { level: 19, dict_size: 16384, gens: 20, pop: 8, time_budget_ms: 30_000 }
    }
}

// Для больших tar (> 8 МБ) расширяем окно до 128 МБ и включаем long-distance matching:
// повторы между файлами всего репозитория находятся напрямую, а не только внутри окна уровня
const BIG_TAR: usize = 8 * 1024 * 1024;
const MAX_WINDOW_LOG: u32 = 27;

fn window_log_for(len: usize) -> Option<u32> {
    if len <= BIG_TAR {
        return None;
    }
    let bits = usize::BITS - (len - 1).leading_zeros();
    Some(bits.clamp(24, MAX_WINDOW_LOG))
}

fn compress_with(data: &[u8], level: i32, dict: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut c = match dict {
        Some(d) => zstd::bulk::Compressor::with_dictionary(level, d),
        None => zstd::bulk::Compressor::new(level),
    }
    .map_err(|e| e.to_string())?;
    if let Some(wl) = window_log_for(data.len()) {
        c.set_parameter(zstd::zstd_safe::CParameter::WindowLog(wl)).map_err(|e| e.to_string())?;
        c.set_parameter(zstd::zstd_safe::CParameter::EnableLongDistanceMatching(true)).map_err(|e| e.to_string())?;
    }
    c.compress(data).map_err(|e| e.to_string())
}

fn decompress_with(frame: &[u8], dict: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let cap = zstd::bulk::Decompressor::upper_bound(frame).unwrap_or(512 * 1024 * 1024);
    let mut d = match dict {
        Some(dict) => zstd::bulk::Decompressor::with_dictionary(dict),
        None => zstd::bulk::Decompressor::new(),
    }
    .map_err(|e| e.to_string())?;
    d.set_parameter(zstd::zstd_safe::DParameter::WindowLogMax(MAX_WINDOW_LOG)).map_err(|e| e.to_string())?;
    d.decompress(frame, cap).map_err(|e| e.to_string())
}

// tar → контейнер XDP2 (magic + u32 LE длина словаря + словарь + zstd-фрейм)
pub fn pack_bytes<F: FnMut(String)>(tar_bytes: &[u8], opts: &PackOptions, mut on_line: F) -> Result<Vec<u8>, String> {
    let samples = tar_samples(tar_bytes);
    let dict = evolve_dictionary(&samples, tar_bytes, opts, &mut on_line);
    if let Some(wl) = window_log_for(tar_bytes.len()) {
        on_line(format!("большой архив: окно 2^{} байт + long-distance matching", wl));
    }
    // Финальная страховка: сравнение со/без словаря на реальном уровне сжатия
    let plain = compress_with(tar_bytes, opts.level, None)?;
    let (dict, compressed) = if dict.is_empty() {
        (Vec::new(), plain)
    } else {
        let with_dict = compress_with(tar_bytes, opts.level, Some(&dict))?;
        if with_dict.len() + dict.len() < plain.len() {
            (dict, with_dict)
        } else {
            (Vec::new(), plain)
        }
    };
    let mut out = Vec::with_capacity(8 + dict.len() + compressed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(dict.len() as u32).to_le_bytes());
    out.extend_from_slice(&dict);
    out.extend_from_slice(&compressed);
    Ok(out)
}

// Контейнер XDP2 (или сырой zstd-фрейм из старых web-экспортов) → tar
pub fn unpack_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 8 || &data[0..4] != MAGIC {
        if data.len() > 4 && data[0] == 0x28 && data[1] == 0xb5 && data[2] == 0x2f && data[3] == 0xfd {
            let cap = zstd::bulk::Decompressor::upper_bound(data).unwrap_or(512 * 1024 * 1024);
            return zstd::bulk::decompress(data, cap).map_err(|e| e.to_string());
        }
        return Err("не XDP2-контейнер".into());
    }
    let dict_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    if data.len() < 8 + dict_len {
        return Err("повреждённый контейнер (длина словаря)".into());
    }
    let dict = &data[8..8 + dict_len];
    let frame = &data[8 + dict_len..];
    decompress_with(frame, if dict_len == 0 { None } else { Some(dict) })
}
