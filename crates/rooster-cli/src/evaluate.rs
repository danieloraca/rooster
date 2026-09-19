use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const MODEL: &str = "jev-latest";
const MAX_ATTEMPTS: usize = 3;

#[derive(Args)]
pub struct EvaluateArgs {
    /// JSON documents to evaluate. Each file is sent as one independent state.
    #[arg(required = true)]
    documents: Vec<PathBuf>,
    /// Emit one machine-readable report instead of terminal-oriented output.
    #[arg(long)]
    json: bool,
    /// Probability at or above which a Noul answer is reported as yes.
    #[arg(long, default_value_t = 0.5)]
    threshold: f64,
    /// TypeSafe API endpoint override, primarily for compatible test services.
    #[arg(long, env = "TYPESAFE_API_URL", default_value = ENDPOINT, hide = true)]
    endpoint: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    model: String,
    answers: Answers,
    usage: Usage,
}

#[derive(Debug, Deserialize)]
struct Answers {
    contains_personal_data: NoulAnswer,
    requires_human_review: NoulAnswer,
}

#[derive(Debug, Deserialize)]
struct NoulAnswer {
    #[serde(rename = "type")]
    answer_type: String,
    noul: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Usage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug, Serialize)]
struct Report {
    model: String,
    threshold: f64,
    documents: Vec<DocumentEvaluation>,
    usage: Usage,
}

#[derive(Debug, Serialize)]
struct DocumentEvaluation {
    path: String,
    contains_personal_data: Decision,
    requires_human_review: Decision,
    usage: Usage,
}

#[derive(Debug, Serialize)]
struct Decision {
    answer: bool,
    probability: f64,
}

pub fn run(args: EvaluateArgs) -> Result<u8, Box<dyn std::error::Error>> {
    if !args.threshold.is_finite() || !(0.0..=1.0).contains(&args.threshold) {
        return Err("--threshold must be a number from 0 to 1".into());
    }
    let api_key = env::var("TYPESAFE_API_KEY")
        .map_err(|_| "TYPESAFE_API_KEY is required to evaluate documents")?;
    if api_key.trim().is_empty() {
        return Err("TYPESAFE_API_KEY cannot be empty".into());
    }

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build();
    let mut report = Report {
        model: MODEL.into(),
        threshold: args.threshold,
        documents: Vec::with_capacity(args.documents.len()),
        usage: Usage::default(),
    };
    for path in args.documents {
        let state = read_document(&path)?;
        let response = evaluate(&agent, &args.endpoint, &api_key, state)?;
        validate_answer(
            "contains_personal_data",
            &response.answers.contains_personal_data,
        )?;
        validate_answer(
            "requires_human_review",
            &response.answers.requires_human_review,
        )?;
        report.model = response.model;
        report.usage.input_tokens += response.usage.input_tokens;
        report.usage.output_tokens += response.usage.output_tokens;
        report.documents.push(DocumentEvaluation {
            path: path.display().to_string(),
            contains_personal_data: decision(
                response.answers.contains_personal_data.noul,
                args.threshold,
            ),
            requires_human_review: decision(
                response.answers.requires_human_review.noul,
                args.threshold,
            ),
            usage: response.usage,
        });
    }

    if args.json {
        let mut output = io::stdout().lock();
        serde_json::to_writer_pretty(&mut output, &report)?;
        writeln!(output)?;
    } else {
        print_human(&report);
    }
    Ok(0)
}

fn read_document(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{} is not valid JSON: {error}", path.display()))?;
    if !matches!(value, Value::String(_) | Value::Array(_) | Value::Object(_)) {
        return Err(format!(
            "{} must contain a JSON string, object, or array",
            path.display()
        )
        .into());
    }
    Ok(value)
}

fn request_body(state: Value) -> Value {
    json!({
        "state": state,
        "model": MODEL,
        "questions": {
            "contains_personal_data": {
                "type": "noul",
                "instructions": "Does this JSON document contain personal data about an identifiable person?",
                "criteria": {
                    "true": "The document contains a direct identifier, contact detail, account or government identifier, location, online identifier, or other information reasonably linkable to a person.",
                    "false": "The document contains no information about an identifiable person; it is anonymous, aggregate, or solely about systems or organizations."
                }
            },
            "requires_human_review": {
                "type": "noul",
                "instructions": "Does this JSON document require review by a human before it is relied on or acted upon?",
                "criteria": {
                    "true": "The document is ambiguous, incomplete, internally inconsistent, exceptional, privacy- or security-sensitive, or could cause a significant consequence if processed incorrectly.",
                    "false": "The document is clear, routine, internally consistent, and suitable for normal automated handling without a person checking it first."
                }
            }
        }
    })
}

fn evaluate(
    agent: &ureq::Agent,
    endpoint: &str,
    api_key: &str,
    state: Value,
) -> Result<ApiResponse, Box<dyn std::error::Error>> {
    let body = request_body(state);
    for attempt in 0..MAX_ATTEMPTS {
        match agent
            .post(endpoint)
            .set("Authorization", &format!("Bearer {api_key}"))
            .send_json(body.clone())
        {
            Ok(response) => return Ok(response.into_json()?),
            Err(ureq::Error::Status(code, response))
                if matches!(code, 429 | 529) && attempt + 1 < MAX_ATTEMPTS =>
            {
                drop(response);
                thread::sleep(Duration::from_millis(200 * (1 << attempt)));
            }
            Err(ureq::Error::Status(code, response)) => {
                let detail = response.into_string().unwrap_or_default();
                let detail = detail.trim();
                return Err(if detail.is_empty() {
                    format!("TypeSafe API returned HTTP {code}").into()
                } else {
                    format!("TypeSafe API returned HTTP {code}: {detail}").into()
                });
            }
            Err(error) => return Err(format!("TypeSafe API request failed: {error}").into()),
        }
    }
    Err("TypeSafe API remained rate-limited or overloaded after 3 attempts".into())
}

fn validate_answer(name: &str, answer: &NoulAnswer) -> Result<(), Box<dyn std::error::Error>> {
    if answer.answer_type != "noul" {
        return Err(format!("TypeSafe returned a non-Noul answer for {name}").into());
    }
    if !answer.noul.is_finite() || !(0.0..=1.0).contains(&answer.noul) {
        return Err(format!("TypeSafe returned an invalid probability for {name}").into());
    }
    Ok(())
}

fn decision(probability: f64, threshold: f64) -> Decision {
    Decision {
        answer: probability >= threshold,
        probability,
    }
}

fn print_human(report: &Report) {
    println!(
        "TypeSafe {} evaluation (yes threshold: {:.2})",
        report.model, report.threshold
    );
    for document in &report.documents {
        println!("\n{}", document.path);
        print_decision("Contains personal data", &document.contains_personal_data);
        print_decision("Requires human review", &document.requires_human_review);
    }
    println!(
        "\nUsage: {} input tokens, {} output tokens",
        report.usage.input_tokens, report.usage.output_tokens
    );
}

fn print_decision(label: &str, decision: &Decision) {
    println!(
        "  {label}: {} ({:.1}% yes probability)",
        if decision.answer { "yes" } else { "no" },
        decision.probability * 100.0
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_request_contains_both_independent_noul_questions() {
        let body = request_body(json!({"name": "Ada", "status": "pending"}));
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["state"]["name"], "Ada");
        assert_eq!(body["questions"]["contains_personal_data"]["type"], "noul");
        assert_eq!(body["questions"]["requires_human_review"]["type"], "noul");
        assert_eq!(body["questions"].as_object().unwrap().len(), 2);
    }

    #[test]
    fn threshold_turns_probability_into_a_boolean_without_discarding_probability() {
        let below = decision(0.79, 0.8);
        let at = decision(0.8, 0.8);
        assert!(!below.answer);
        assert_eq!(below.probability, 0.79);
        assert!(at.answer);
    }

    #[test]
    fn response_validation_rejects_wrong_types_and_probabilities() {
        assert!(
            validate_answer(
                "test",
                &NoulAnswer {
                    answer_type: "choice".into(),
                    noul: 0.5,
                },
            )
            .is_err()
        );
        assert!(
            validate_answer(
                "test",
                &NoulAnswer {
                    answer_type: "noul".into(),
                    noul: 1.1,
                },
            )
            .is_err()
        );
    }
}
