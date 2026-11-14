// xdp-util: CLI-обёртка над библиотекой xdp_util (используется Node-backend'ом web-версии).
// Ядро формата — в lib.rs, то же самое ядро использует Tauri desktop app.
use std::env;
use std::fs;
use std::process::exit;
use xdp_util::{evolve_dictionary, pack_bytes, tar_samples, unpack_bytes, PackOptions};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage();
    }
    let cmd = args[1].as_str();
    let opts = parse_opts(&args[2..]);
    let result = match cmd {
        "pack" => pack(&opts),
        "unpack" => unpack(&opts),
        "train-dict" => train_dict(&opts),
        _ => usage(),
    };
    if let Err(e) = result {
        eprintln!("ошибка: {}", e);
        exit(1);
    }
}

fn usage() -> ! {
    eprintln!(
        "xdp-util — утилита .xdp (ZSTD + генетический словарь)\n\n\
         Команды:\n\
         \x20 pack       --input <tar> --output <xdp> [--level 19] [--dict-size 16384] [--gens 20] [--pop 8] [--budget-ms 30000]\n\
         \x20 unpack     --input <xdp> --output <tar>\n\
         \x20 train-dict --input <tar> --output <dict> [--dict-size 16384] [--gens 20] [--pop 8] [--budget-ms 30000]"
    );
    exit(2);
}

struct Opts {
    input: String,
    output: String,
    pack: PackOptions,
}

fn parse_opts(args: &[String]) -> Opts {
    let mut o = Opts { input: String::new(), output: String::new(), pack: PackOptions::default() };
    let mut i = 0;
    while i + 1 < args.len() {
        match args[i].as_str() {
            "--input" => o.input = args[i + 1].clone(),
            "--output" => o.output = args[i + 1].clone(),
            "--level" => o.pack.level = args[i + 1].parse().unwrap_or(19),
            "--dict-size" => o.pack.dict_size = args[i + 1].parse().unwrap_or(16384),
            "--gens" => o.pack.gens = args[i + 1].parse().unwrap_or(20),
            "--pop" => o.pack.pop = args[i + 1].parse().unwrap_or(8),
            "--budget-ms" => o.pack.time_budget_ms = args[i + 1].parse().unwrap_or(30_000),
            _ => {}
        }
        i += 2;
    }
    if o.input.is_empty() || o.output.is_empty() {
        usage();
    }
    o
}

fn pack(o: &Opts) -> Result<(), String> {
    let tar_bytes = fs::read(&o.input).map_err(|e| format!("чтение {}: {}", o.input, e))?;
    let out = pack_bytes(&tar_bytes, &o.pack, |line| eprintln!("{}", line))?;
    fs::write(&o.output, &out).map_err(|e| format!("запись {}: {}", o.output, e))?;
    let dict_len = u32::from_le_bytes([out[4], out[5], out[6], out[7]]);
    eprintln!(
        "pack: tar {} → xdp {} (словарь {} байт, zstd level {})",
        tar_bytes.len(),
        out.len(),
        dict_len,
        o.pack.level
    );
    Ok(())
}

fn unpack(o: &Opts) -> Result<(), String> {
    let data = fs::read(&o.input).map_err(|e| format!("чтение {}: {}", o.input, e))?;
    let tar_bytes = unpack_bytes(&data)?;
    fs::write(&o.output, &tar_bytes).map_err(|e| format!("запись {}: {}", o.output, e))?;
    eprintln!("unpack: xdp {} → tar {}", data.len(), tar_bytes.len());
    Ok(())
}

fn train_dict(o: &Opts) -> Result<(), String> {
    let tar_bytes = fs::read(&o.input).map_err(|e| format!("чтение {}: {}", o.input, e))?;
    let samples = tar_samples(&tar_bytes);
    let dict = evolve_dictionary(&samples, &tar_bytes, &o.pack, |line| eprintln!("{}", line));
    if dict.is_empty() {
        return Err("недостаточно данных для обучения словаря (или словарь не даёт выигрыша)".into());
    }
    fs::write(&o.output, &dict).map_err(|e| format!("запись {}: {}", o.output, e))?;
    eprintln!("train-dict: словарь {} байт из {} сэмплов", dict.len(), samples.len());
    Ok(())
}
