# Task Completion Checklist

When a coding task is completed, run the following checks:

1. **Format code:**
   ```bash
   cargo fmt --all
   ```

2. **Run clippy (linter):**
   ```bash
   cargo clippy --workspace
   ```
   Fix any warnings before committing.

3. **Run tests:**
   ```bash
   cargo test --workspace
   ```
   All tests must pass.

4. **Build check:**
   ```bash
   cargo build
   ```
   Ensure the project compiles without errors.

5. **Commit** using the `/commit` skill (conventional commits with change analysis).
