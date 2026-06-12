import sys

path = r'C:\Users\lilu\works\rflux\.github\workflows\ci.yml'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# Insert cargo audit step after Rust tests
old_marker = '''      - name: Rust tests
        run: |
          export PYO3_PYTHON=\"\C:\Users\lilu\works\rflux\.venv\Scripts\python.exe\"
          cargo test --workspace'''

new_steps = '''      - name: Rust tests
        run: |
          export PYO3_PYTHON=\"\C:\Users\lilu\works\rflux\.venv\Scripts\python.exe\"
          cargo test --workspace

      - name: Rust security audit
        run: |
          cargo install cargo-audit --locked 2>/dev/null || true
          cargo audit 2>&1 | head -100 || true
        continue-on-error: true

      - name: Rust dependency check
        run: |
          cargo install cargo-deny --locked 2>/dev/null || true
          cargo deny check bans licenses sources 2>&1 | head -100 || true
        continue-on-error: true'''

if new_steps not in content:
    content = content.replace(old_marker, new_steps)
    # Also add ruff check after Python tests
    old_marker2 = '''      - name: Python tests
        run: uv run pytest'''

    new_steps2 = '''      - name: Python tests
        run: uv run pytest

      - name: Python lint check
        run: uv run ruff check python/'''

    if new_steps2 not in content:
        content = content.replace(old_marker2, new_steps2)

    with open(path, 'w', encoding='utf-8', newline='\n') as f:
        f.write(content)
    print('Updated CI with audit + deny + ruff checks')
else:
    print('CI already up to date')
