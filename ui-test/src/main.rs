use walkdir::WalkDir;
use forth3::{leakbox::{LBForthParams, LBForth}, Forth};

fn main() {
    let interesting = WalkDir::new("ui-tests")
        .into_iter()
        .filter_map(|e| match e {
            Ok(e) if e.file_type().is_file() => e.file_name().to_str().and_then(|name| {
                if name.ends_with(".fth") {
                    Some(e.clone())
                } else {
                    None
                }
            }),
            _ => None,
        });

    for entry in interesting {
        println!("{}", entry.path().display());
        let contents = std::fs::read_to_string(entry.path()).unwrap();
        let tokd = tokenize(contents).unwrap();
        let mut forth = LBForth::from_params(tokd.settings, (), Forth::FULL_BUILTINS);

        for Step { input, output } in tokd.steps.into_iter() {
            forth.forth.input.fill(&input).unwrap();
            let res = forth.forth.process_line();
            match (res, output) {
                (Ok(()), Outcome::OkAnyOutput) => {}
                (Ok(()), Outcome::OkWithOutput(exp)) => {
                    let act_lines = forth.forth.output.as_str().lines().collect::<Vec<&str>>();
                    assert_eq!(act_lines.len(), exp.len());
                    act_lines.iter().zip(exp.iter()).for_each(|(a, e)| {
                        assert_eq!(a.trim_end(), e.trim_end());
                    })
                }
                (Err(_e), Outcome::FatalError) => {}
                (res, exp) => {
                    eprintln!("Error!");
                    eprintln!("Expected: {exp:?}");
                    eprintln!("Got: {res:?}");
                    if res.is_ok() {
                        eprintln!("Output:\n{}", forth.forth.output.as_str());
                    }
                    panic!();
                }
            }
            forth.forth.output.clear();
        }
    }
}

#[derive(Debug)]
enum Outcome {
    OkAnyOutput,
    OkWithOutput(Vec<String>),
    FatalError,
}

#[derive(Debug)]
struct Step {
    input: String,
    output: Outcome,
}

#[derive(Default, Debug)]
struct Tokenized {
    settings: LBForthParams,
    steps: Vec<Step>,
}

fn tokenize(contents: String) -> Result<Tokenized, ()> {
    let mut lines = contents.lines();
    let mut output = Tokenized::default();
    let mut frontmatter_done = false;

    while let Some(line) = lines.next() {
        let (tok, remain) = if let Some(t) = line.split_once(" ") {
            t
        } else {
            continue;
        };

        match tok {
            ">" => {
                frontmatter_done = true;
                output.steps.push(Step {
                    input: remain.to_string(),
                    output: Outcome::OkAnyOutput,
                });
            }
            "<" => {
                frontmatter_done = true;
                let cur_step = output.steps.last_mut().unwrap();
                let expected_out = remain.to_string();
                match &mut cur_step.output {
                    Outcome::OkAnyOutput => {
                        cur_step.output = Outcome::OkWithOutput(vec![expected_out]);
                    },
                    Outcome::OkWithOutput(o) => {
                        o.push(remain.to_string());
                    },
                    Outcome::FatalError => panic!("Fatal error can't set output"),
                }
            }
            "x" => {
                frontmatter_done = true;
                output.steps.push(Step {
                    input: remain.to_string(),
                    output: Outcome::FatalError,
                });
            }
            "(" => {
                assert!(!frontmatter_done);
                let mut split = remain.split_whitespace();
                let mut is_comment = false;
                match split.next() {
                    Some("data_stack_elems") => {
                        output.settings.data_stack_elems = split.next().unwrap().parse::<usize>().unwrap();
                    }
                    Some("return_stack_elems") => {
                        output.settings.return_stack_elems = split.next().unwrap().parse::<usize>().unwrap();
                    }
                    Some("control_stack_elems") => {
                        output.settings.control_stack_elems = split.next().unwrap().parse::<usize>().unwrap();
                    }
                    Some("input_buf_elems") => {
                        output.settings.input_buf_elems = split.next().unwrap().parse::<usize>().unwrap();
                    }
                    Some("output_buf_elems") => {
                        output.settings.output_buf_elems = split.next().unwrap().parse::<usize>().unwrap();
                    }
                    Some("dict_buf_elems") => {
                        output.settings.dict_buf_elems = split.next().unwrap().parse::<usize>().unwrap();
                    }
                    Some(_) => {
                        is_comment = true;
                    }
                    _ => panic!(),
                }
                if !is_comment {
                    assert_eq!(Some(")"), split.next());
                }
            }
            _ => {}
        }
    }


    Ok(output)
}
