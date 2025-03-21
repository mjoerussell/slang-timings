use std::{collections::HashMap, fmt::{Display, Write}, path::Path};

use slang_solidity::{compilation::{AddFileResponse, InternalCompilationBuilder}, cst::{Cursor, TerminalKind}};
use semver::Version;

type Result<T> = std::result::Result<T, String>;

struct RunConfig {
    command_name: String,
    show_help: bool,
    list_available_cases: bool,
    cases: Vec<String>,
}

impl RunConfig {
    fn new(args: Vec<String>) -> RunConfig {
        let mut config = RunConfig{
            command_name: args[0].to_owned(),
            show_help: false,
            list_available_cases: false,
            cases: vec![],
        };

        for arg in args.iter().skip(1) {
            if arg == "-l" || arg == "--list" {
                config.list_available_cases = true;
            } else if arg == "-h" || arg == "--help" {
                config.show_help = true;
            } else {
                config.cases.push(arg.to_owned());
            }
        }

        config
    }
}

fn main() {
    let cases = setup_test_cases();
    let config = RunConfig::new(std::env::args().collect());

    if config.show_help {
        println!("Usage: {} [-h | -l] [TestCases]", config.command_name);
        return;
    }

    if config.list_available_cases {
        println!("Available test cases:");
        for test_case in cases.values() {
            println!("{} ({})", test_case.name, test_case.path);
        }
        return;
    }

    let mut results = vec![];

    if config.cases.is_empty() {
        // Run all test cases when none are specified
        for test_case in cases.values() {
            println!("[{}] Starting test", test_case.name);

            match test_case.run() {
                Ok(result) => results.push(result),
                Err(err) => {
                    println!("[{}] Run failed: {err}", test_case.name);
                }
            }
        }
    } else {
        // Run only specified test cases
        for case_name in config.cases {
            if let Some(test_case) = cases.get(&case_name) {
                println!("[{}] Starting test", test_case.name);

                match test_case.run() {
                    Ok(result) => results.push(result),
                    Err(err) => {
                        println!("[{}] Run failed: {err}", test_case.name);
                    }
                }
            } else {
                println!("Did not find test case \"{case_name}\"");
            }
        }
    }

    std::fs::write("results.csv", results_to_csv(results)).unwrap();
}

struct CompilationBuilder {
    internal: InternalCompilationBuilder,
    seen_files: Vec<String>,
}

impl CompilationBuilder {
    fn create(lang_version: Version) -> CompilationBuilder {
        CompilationBuilder {
            internal: InternalCompilationBuilder::create(lang_version).unwrap(),
            seen_files: vec![],
        }
    }

    /// Add a new file to the compilation unit.
    fn add_file(&mut self, file: &str) -> Result<()> {
        if self.has_seen_file(file) {
            return Ok(());
        }

        let path: String = file.into();

        self.seen_files.push(path.clone());
        
        let contents = if let Ok(contents) = std::fs::read_to_string(file) {
            contents
        } else {
            return Err(format!("Could not open file {file}"));
        };

        let AddFileResponse { import_paths } = self.internal.add_file(path, &contents);

        for import_path in import_paths {
            let file_id = resolve_path(file, &import_path)?; 
            if self.internal.resolve_import(file, &import_path, file_id.clone()).is_err() {
                return Err("Can't resolve import".into());
            }
            self.add_file(&file_id)?;
        }

        Ok(())
    }

    fn build(&mut self) -> slang_solidity::compilation::CompilationUnit {
        self.internal.build()
    }

    fn has_seen_file(&self, file: &str) -> bool {
        self.seen_files.iter().any(|f| f.as_str() == file)
    }
}

fn resolve_path(context_path: &str, path_to_resolve: &Cursor) -> Result<String> {
    let path = path_to_resolve.node().unparse();
    let path = path
        .strip_prefix(|c| matches!(c, '"' | '\''))
        .unwrap()
        .strip_suffix(|c| matches!(c, '"' | '\''))
        .unwrap();

    let context_path = if let Some(context_path) = Path::new(context_path).parent() {
        context_path
    } else {
        return Err(format!("Could not get parent of context_path: {context_path}"));
    };

    let path = if let Ok(path) = context_path.join(path).canonicalize() {
        path
    } else {
        return Err(format!("Could not canonicalize path: {path}"));
    };

    let path_str = if let Some(path_str) = path.as_os_str().to_str() {
        path_str
    } else {
        return Err("Could not convert path to str".into());
    };

    if path.exists() {
        Ok(String::from(path_str))
    } else {
        Err(format!("Path does not exist: {path_str}"))
    }
}

#[derive(Default, Debug)]
struct TestResult {
    name: String,
    total_time: usize,
    build_time: usize,
    setup_time: usize,
    resolution_time: usize,
    goto_times: Vec<usize>,
}

impl TestResult {
    fn max_goto(&self) -> usize {
        if let Some(max) = self.goto_times.iter().max() {
            *max
        } else {
            0
        }
    }

    fn min_goto(&self) -> usize {
        if let Some(min) = self.goto_times.iter().min() {
            *min
        } else {
            0
        }
    }

    fn mean_goto(&self) -> f32 {
        let sum = self.goto_times.iter().sum::<usize>() as f32;
        sum / (self.goto_times.len() as f32)
    }
}

impl Display for TestResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{},{},{},{},{},{},{},{}", self.name, self.total_time as f32 / 1000.0, self.build_time  as f32 / 1000.0, self.setup_time as f32 / 1000.0, self.resolution_time as f32 / 1000.0, self.max_goto(), self.min_goto(), self.mean_goto())
    }
}

fn results_to_csv(results: Vec<TestResult>) -> String {
    let mut csv = String::new();

    writeln!(&mut csv, "Name,Total Time,Build Time,Setup Time,Resolution Time,Go to Def/Ref (max),Go to Def/Ref (min),Go to Def/Ref (mean)").unwrap();

    for res in results {
        writeln!(&mut csv, "{}", res).unwrap();
    }

    csv
}

struct TestCase {
    name: String,
    path: String,
    version: Version,
}

impl TestCase {
    fn new(name: String, path: String, version: Version) -> TestCase {
        TestCase {
            name,
            path,
            version
        }
    }

    fn run(&self) -> Result<TestResult> {
        let mut result = TestResult{
            name: self.name.clone(),
            ..Default::default()
        };

        let total_start = std::time::Instant::now();
        let mut builder = CompilationBuilder::create(self.version.clone());
        builder.add_file(&self.path)?;
    
        let setup_start = std::time::Instant::now();
        let unit = builder.build();
        let setup_end = std::time::Instant::now();

        result.setup_time = (setup_end - setup_start).as_millis() as usize;

        let mut cursor = unit.file(&self.path).unwrap().create_tree_cursor();

        let build_start = std::time::Instant::now();
        unit.binding_graph();
        let build_end = std::time::Instant::now();

        result.build_time = (build_end - build_start).as_millis() as usize;

        while cursor.go_to_next_terminal_with_kind(TerminalKind::Identifier) {
            let goto_start = std::time::Instant::now();
            unit.binding_graph().definition_at(&cursor);
            if let Some(reference) = unit.binding_graph().reference_at(&cursor) {
                reference.definitions().len();
            }
            let goto_end = std::time::Instant::now();

            let goto_time = (goto_end - goto_start).as_millis() as usize;

            result.goto_times.push(goto_time);
        }

        let total_end = std::time::Instant::now();

        result.total_time = (total_end - total_start).as_millis() as usize;
        result.resolution_time = result.total_time - result.build_time - result.setup_time;

        Ok(result)
    }
}

fn setup_test_cases() -> HashMap<String, TestCase> {
    let mut cases = HashMap::new();

    add_test_case(
        &mut cases,
        "WeightedPool", 
        "sol-sources/0x01abc00E86C7e258823b9a055Fd62cA6CF61a163/sources/contracts/pools/weighted/WeightedPool.sol", 
        Version::new(0, 7, 6)
    );

    add_test_case(
        &mut cases,
        "DoodledBears", 
        "sol-sources/0x015E220901014BAE4f7e168925CD74e725e23692/sources/DoodledBears.sol", 
        Version::new(0, 8, 11)
    );

    add_test_case(
        &mut cases,
        "UiPoolDataProviderV2V3", 
        "sol-sources/0x00e50FAB64eBB37b87df06Aa46b8B35d5f1A4e1A/contracts/misc/UiPoolDataProviderV2V3.sol", 
        Version::new(0, 6, 12)
    );

    add_test_case(
        &mut cases,
        "ERC721AContract", 
        "sol-sources/0x01665987bC6725070e56d160d75AA19d8B73273e/sources/project:/contracts/ERC721AContract.sol", 
        Version::new(0, 8, 9)
    );

    add_test_case(
        &mut cases,
        "SeniorBond", 
        "sol-sources/0x0170f38fa8df1440521c8b8520BaAd0CdA132E82/sources/contracts/SeniorBond.sol", 
        Version::new(0, 7, 6)
    );

    add_test_case(
        &mut cases,
        "Mooniswap", 
        "sol-sources/0x01a11a5A999E57E1B177AA2fF7fEA957605adA2b/sources/Users/k06a/Projects/mooniswap-v2/contracts/Mooniswap.sol", 
        Version::new(0, 6, 12)
    );

    add_test_case(
        &mut cases,
        "Darts", 
        "sol-sources/0x01a5E3268E3987f0EE5e6Eb12fe63fa2AF992D83/sources/contracts/Darts.sol", 
        Version::new(0, 8, 0)
    );

    add_test_case(
        &mut cases,
        "YaxisVotePower", 
        "sol-sources/0x01fef0d5d6fd6b5701ae913cafb11ddaee982c9a/YaxisVotePower/contracts/governance/YaxisVotePower.sol", 
        Version::new(0, 7, 0)
    );

    add_test_case(
        &mut cases,
        "0xProject", 
        "sol-sources/0xProject/contracts/governance/src/ZeroExProtocolGovernor.sol", 
        Version::new(0, 8, 19)
    );
    
    add_test_case(
        &mut cases,
        "Uniswap", 
        "sol-sources/Uniswap/contracts/UniswapV3Factory.sol", 
        Version::new(0, 7, 6)
    );

    add_test_case(
        &mut cases,
        "AAVE", 
        "sol-sources/aave-v3-core-master/contracts/protocol/pool/Pool.sol", 
        Version::new(0, 8, 10)
    );
    
    add_test_case(
        &mut cases,
        "GraphToken", 
        "sol-sources/graph_protocol/contracts/token/GraphToken.sol", 
        Version::new(0, 7, 6)
    );

    add_test_case(
        &mut cases,
        "lidofinance",
        "sol-sources/lidofinance/contracts/0.8.9/WithdrawalQueueERC721.sol",
        Version::new(0, 8, 9)
    );

    cases
}

fn add_test_case(cases: &mut HashMap<String, TestCase>, name: &str, path: &str, version: Version) {
    cases.insert(
        name.into(), 
        TestCase::new(
            name.into(),
            path.into(), 
            version
        )
    );
}


