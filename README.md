# ChemSpec

ChemSpec is a chemistry exploration app built for the Education category of [OpenAI Build Week 2026](https://openai.devpost.com/). Learners construct a reaction question, follow its structural changes atom by atom, and then see a human-level 3D interpretation of the same outcome.

<img alt="the ChemSpec dashboard" src="https://github.com/user-attachments/assets/ee612b0e-f844-4865-9563-1e38ad9371e4" />

## Running ChemSpec

### Web demo

Open the [ChemSpec web demo](https://charles-mills.github.io/ChemSpec/) in a browser with WebGPU support. The demo excludes Codex integration and may not work on your device.

### Desktop app

Install the latest [release](https://github.com/charles-mills/ChemSpec/releases) for your platform.

### From Source

With [rustup](https://rustup.rs/) available, run the following: 

```sh
git clone https://github.com/charles-mills/ChemSpec.git
cd ChemSpec
cargo run -p chemspec-app
```

Local chemistry does not require an account or network connection. To use the LLM path, install the [Codex CLI](https://github.com/openai/codex), sign in with a ChatGPT account and relaunch ChemSpec.

