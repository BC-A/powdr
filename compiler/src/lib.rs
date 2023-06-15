//! The main powdr lib, used to compile from assembly to PIL

use std::ffi::OsStr;
use std::fs;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;

mod backends;
mod verify;

pub use backends::Backend;
use number::write_polys_file;
use pil_analyzer::json_exporter;
pub use verify::{compile_asm_string_temp, verify, verify_asm_string};

use executor::constant_evaluator;
use number::FieldElement;
use parser::ast::PILFile;

pub fn no_callback<T>() -> Option<fn(&str) -> Option<T>> {
    None
}

/// Compiles a .pil or .asm file and runs witness generation.
/// If the file ends in .asm, converts it to .pil first.
pub fn compile_pil_or_asm<T: FieldElement>(
    file_name: &str,
    inputs: Vec<T>,
    output_dir: &Path,
    force_overwrite: bool,
    prove_with: Option<Backend>,
) {
    if file_name.ends_with(".asm") {
        compile_asm(file_name, inputs, output_dir, force_overwrite, prove_with)
    } else {
        compile_pil(
            Path::new(file_name),
            output_dir,
            Some(inputs_to_query_callback(inputs)),
            prove_with,
        );
    };
}

/// Compiles a .pil file to its json form and also tries to generate
/// constants and committed polynomials.
/// @returns true if all committed/witness and constant/fixed polynomials
/// could be generated.
pub fn compile_pil<T: FieldElement, QueryCallback>(
    pil_file: &Path,
    output_dir: &Path,
    query_callback: Option<QueryCallback>,
    prove_with: Option<Backend>,
) -> bool
where
    QueryCallback: FnMut(&str) -> Option<T> + Sync + Send,
{
    compile(
        &pil_analyzer::analyze(pil_file),
        pil_file.file_name().unwrap(),
        output_dir,
        query_callback,
        prove_with,
    )
}

pub fn compile_pil_ast<T: FieldElement, QueryCallback>(
    pil: &PILFile<T>,
    file_name: &OsStr,
    output_dir: &Path,
    query_callback: Option<QueryCallback>,
    prove_with: Option<Backend>,
) -> bool
where
    QueryCallback: FnMut(&str) -> Option<T> + Sync + Send,
{
    // TODO exporting this to string as a hack because the parser
    // is tied into the analyzer due to imports.
    compile(
        &pil_analyzer::analyze_string(&format!("{pil}")),
        file_name,
        output_dir,
        query_callback,
        prove_with,
    )
}

/// Compiles a .asm file, outputs the PIL on stdout and tries to generate
/// fixed and witness columns.
pub fn compile_asm<T: FieldElement>(
    file_name: &str,
    inputs: Vec<T>,
    output_dir: &Path,
    force_overwrite: bool,
    prove_with: Option<Backend>,
) {
    let contents = fs::read_to_string(file_name).unwrap();
    compile_asm_string(
        file_name,
        &contents,
        inputs,
        output_dir,
        force_overwrite,
        prove_with,
    )
}

/// Compiles the contents of a .asm file, outputs the PIL on stdout and tries to generate
/// fixed and witness columns.
pub fn compile_asm_string<T: FieldElement>(
    file_name: &str,
    contents: &str,
    inputs: Vec<T>,
    output_dir: &Path,
    force_overwrite: bool,
    prove_with: Option<Backend>,
) {
    let pil = pilgen::compile(Some(file_name), contents).unwrap_or_else(|err| {
        eprintln!("Error parsing .asm file:");
        err.output_to_stderr();
        panic!();
    });
    let pil_file_name = output_dir.join(format!(
        "{}.pil",
        Path::new(file_name).file_stem().unwrap().to_str().unwrap()
    ));
    if pil_file_name.exists() && !force_overwrite {
        eprint!(
            "Target file {} already exists. Not overwriting.",
            pil_file_name.to_str().unwrap()
        );
        return;
    }
    fs::write(pil_file_name.clone(), format!("{pil}")).unwrap();

    compile_pil_ast(
        &pil,
        pil_file_name.file_name().unwrap(),
        output_dir,
        Some(inputs_to_query_callback(inputs)),
        prove_with,
    );
}

fn compile<T: FieldElement, QueryCallback>(
    analyzed: &pil_analyzer::Analyzed<T>,
    file_name: &OsStr,
    output_dir: &Path,
    query_callback: Option<QueryCallback>,
    prove_with: Option<Backend>,
) -> bool
where
    QueryCallback: FnMut(&str) -> Option<T> + Send + Sync,
{
    let mut success = true;
    let start = Instant::now();
    log::info!("Evaluating fixed columns...");
    let (constants, degree) = constant_evaluator::generate(analyzed);
    log::info!("Took {}", start.elapsed().as_secs_f32());
    if analyzed.constant_count() == constants.len() {
        write_polys_file(
            &mut BufWriter::new(&mut fs::File::create(output_dir.join("constants.bin")).unwrap()),
            degree,
            &constants,
        );
        log::info!("Wrote constants.bin.");
        log::info!("Deducing witness columns...");
        let commits = executor::witgen::generate(analyzed, degree, &constants, query_callback);
        write_polys_file(
            &mut BufWriter::new(&mut fs::File::create(output_dir.join("commits.bin")).unwrap()),
            degree,
            &commits,
        );
        log::info!("Wrote commits.bin.");
        if let Some(Backend::Halo2) = prove_with {
            use std::io::Write;
            let proof = halo2::prove_ast(analyzed, constants, commits);
            let mut proof_file = fs::File::create(output_dir.join("proof.bin")).unwrap();
            let mut proof_writer = BufWriter::new(&mut proof_file);
            proof_writer.write_all(&proof).unwrap();
            proof_writer.flush().unwrap();
            log::info!("Wrote proof.bin.");
        }
    } else {
        log::warn!("Not writing constants.bin because not all declared constants are defined (or there are none).");
        success = false;
    }
    let json_out = json_exporter::export(analyzed);
    let json_file = {
        let mut file = file_name.to_os_string();
        file.push(".json");
        file
    };
    json_out
        .write(&mut fs::File::create(output_dir.join(&json_file)).unwrap())
        .unwrap();
    log::info!("Wrote {}.", json_file.to_string_lossy());

    success
}

fn inputs_to_query_callback<T: FieldElement>(inputs: Vec<T>) -> impl Fn(&str) -> Option<T> {
    move |query: &str| -> Option<T> {
        let items = query.split(',').map(|s| s.trim()).collect::<Vec<_>>();
        match items[0] {
            "\"input\"" => {
                assert_eq!(items.len(), 2);
                let index = items[1].parse::<usize>().unwrap();
                let value = inputs.get(index).cloned();
                if let Some(value) = value {
                    log::trace!("Input query: Index {index} -> {value}");
                }
                value
            }
            "\"print_char\"" => {
                assert_eq!(items.len(), 2);
                print!("{}", items[1].parse::<u8>().unwrap() as char);
                Some(0.into())
            }
            _ => None,
        }
    }
}
