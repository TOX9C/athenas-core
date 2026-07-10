# Development Guide

> This document extends the project's private global rules by providing a clear,
> project-specific usage guide for the Rust codebase.
>
> It aligns with:
> - [/claude/rules/common/development-workflow.md](../.claude/rules/common/development-workflow.md) — the feature development pipeline.
> - [/claude/rules/common/agents.md](../.claude/rules/common/agents.md) — available agent orchestration.
> - [/claude/rules/common/testing.md](../.claude/rules/common/testing.md) — testing requirements.
> - [/claude/rules/common/security.md](../.claude/rules/common/security.md) — security guidelines.

## Prerequisites

Ensure you have the following tools installed and configured:

- **Rust Toolchain**: `rustfmt`, `clippy` (install via `rustup component add rustfmt clippy`)
- **cargo-llvm-cov**: For coverage reporting (`cargo install cargo-llvm-cov`)

## Development Workflow

Follow the standard feature implementation workflow defined in the common rules:

1. **Research & Reuse**: Search for existing implementations and battle-tested skeletons before writing new code.
2. **Plan First**: Use the `planner` agent to create an implementation plan before coding.
3. **TDD Approach**: Use the `tdd-guide` agent. Write tests first, then implement.
4. **Code Review**: Use the `code-reviewer` agent, or for Rust, the `rust-reviewer` agent.
5. **Commit & Push**: Follow the commit message format (`feat:`, `fix:`, `refactor:`, etc.).

## Rust Standards

This project enforces strict Rust standards derived from the private global rules in `.claude/rules/rust/`.

### Style & Formatting

- Run `cargo fmt` before every commit.
- Run `cargo clippy -- -D warnings` to treat warnings as errors.
- Max line width: 100 characters.
- **Immutability by Default**: Use `let` by default. Return new values rather than mutating in place.

### Error Handling

- Use `Result<T, E>` and `?` for propagation.
- Prefer `thiserror` for libraries and `anyhow` for applications.
- Never use `unwrap()` in production code. Use `.expect()` or `.with_context()` instead.

### Ownership & Borrowing

- Borrow (`&T`) by default; take ownership only when necessary.
- Accept `&str` over `String`, `&[T]` over `Vec<T>` in function parameters.
- Never clone to satisfy the borrow checker without understanding the root cause.

### Testing

- Maintain **80%+ line coverage**.
- Use `#[test]` with `#[cfg(test)]` modules for unit tests.
- Use `rstest` for parameterized tests.
- Use `mockall` for trait-based mocking.
- Use `#[tokio::test]` for async tests.

Example:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_user_with_valid_email() {
        let user = User::new("Alice", "alice@example.com").unwrap();
        assert_eq!(user.name, "Alice");
    }
}
```

### Security

Refer to the full [security guidelines](../.claude/rules/common/security.md) and [Rust security rules](../.claude/rules/rust/security.md). Key points:

- No hardcoded secrets. Use `std::env::var("API_KEY")`.
- Always use parameterized queries to prevent SQL injection.
- Validate all user input at system boundaries.
- Minimize `unsafe` blocks; every block must have a `// SAFETY:` comment.
- Run `cargo audit` regularly to scan for CVEs.

## Agent Usage

Utilize the following agents during development, as defined in the common rules:

| Agent | Purpose | When to Use |
|-------|---------|-------------|
| `planner` | Implementation planning | Complex features, refactoring |
| `tdd-guide` | Test-driven development | New features, bug fixes |
| `code-reviewer` | Code review | After writing code |
| `security-reviewer` | Security analysis | Before commits |
| `rust-reviewer` | Rust-specific review | After writing Rust code |
| `build-error-resolver` | Fix build errors | When `cargo build` fails |

### Parallel Execution

For independent tasks, always execute agents in parallel to save time. For example, when reviewing a new feature:

1. Launch `rust-reviewer` for code quality.
2. Launch `security-reviewer` for vulnerability checks.
3. Launch `tdd-guide` to verify test coverage.

## Project Structure

Organize code by domain, not by type:

```text
src/
├── main.rs
├── lib.rs
├── auth/           # Domain module
│   ├── mod.rs
│   ├── token.rs
│   └── middleware.rs
├── orders/         # Domain module
│   ├── mod.rs
│   ├── model.rs
│   └── service.rs
└── db/             # Infrastructure
    ├── mod.rs
    └── pool.rs
```