# Graph Report - shell-rust  (2026-07-31)

## Corpus Check
- 7 files · ~4,804 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 36 nodes · 77 edges · 6 communities detected
- Extraction: 60% EXTRACTED · 40% INFERRED · 0% AMBIGUOUS · INFERRED: 31 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Community 0|Community 0]]
- [[_COMMUNITY_Community 1|Community 1]]
- [[_COMMUNITY_Community 2|Community 2]]
- [[_COMMUNITY_Community 3|Community 3]]
- [[_COMMUNITY_Community 4|Community 4]]
- [[_COMMUNITY_Community 5|Community 5]]

## God Nodes (most connected - your core abstractions)
1. `run_command()` - 12 edges
2. `run_builtin()` - 11 edges
3. `run_pipeline()` - 10 edges
4. `find_executable()` - 6 edges
5. `reap()` - 5 edges
6. `is_builtin()` - 4 edges
7. `reap_background_jobs()` - 4 edges
8. `parse_redirections()` - 4 edges
9. `emit()` - 4 edges
10. `home_dir()` - 4 edges

## Surprising Connections (you probably didn't know these)
- `run_builtin()` --calls--> `emit()`  [INFERRED]
  src/builtins.rs → src/redirection.rs
- `run_builtin()` --calls--> `home_dir()`  [INFERRED]
  src/builtins.rs → src/path.rs
- `run_builtin()` --calls--> `find_executable()`  [INFERRED]
  src/builtins.rs → src/path.rs
- `run_builtin()` --calls--> `reap()`  [INFERRED]
  src/builtins.rs → src/jobs.rs
- `run_pipeline()` --calls--> `parse_redirections()`  [INFERRED]
  src/builtins.rs → src/redirection.rs

## Communities

### Community 0 - "Community 0"
Cohesion: 0.44
Nodes (7): create_pipe(), is_builtin(), open_redirect(), print_job(), run_builtin(), run_builtin_into_pipe(), run_pipeline()

### Community 1 - "Community 1"
Cohesion: 0.43
Nodes (4): longest_common_prefix(), ShellHelper, common_prefix(), completions()

### Community 2 - "Community 2"
Cohesion: 0.5
Nodes (4): run_command(), emit(), parse_redirections(), Redirection

### Community 3 - "Community 3"
Cohesion: 0.6
Nodes (4): executables_starting_with(), find_executable(), home_dir(), is_executable()

### Community 4 - "Community 4"
Cohesion: 0.4
Nodes (4): add_job(), Job, JobSnapshot, JobStatus

### Community 5 - "Community 5"
Cohesion: 0.5
Nodes (3): reap_background_jobs(), reap(), main()

## Knowledge Gaps
- **4 isolated node(s):** `Redirection`, `JobStatus`, `JobSnapshot`, `Job`
  These have ≤1 connection - possible missing edges or undocumented components.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `run_command()` connect `Community 2` to `Community 0`, `Community 1`, `Community 3`, `Community 4`, `Community 5`?**
  _High betweenness centrality (0.147) - this node is a cross-community bridge._
- **Why does `reap()` connect `Community 5` to `Community 0`, `Community 1`, `Community 2`, `Community 4`?**
  _High betweenness centrality (0.140) - this node is a cross-community bridge._
- **Why does `run_builtin()` connect `Community 0` to `Community 1`, `Community 2`, `Community 3`, `Community 5`?**
  _High betweenness centrality (0.122) - this node is a cross-community bridge._
- **Are the 8 inferred relationships involving `run_command()` (e.g. with `.new()` and `parse_redirections()`) actually correct?**
  _`run_command()` has 8 INFERRED edges - model-reasoned connections that need verification._
- **Are the 5 inferred relationships involving `run_builtin()` (e.g. with `.new()` and `emit()`) actually correct?**
  _`run_builtin()` has 5 INFERRED edges - model-reasoned connections that need verification._
- **Are the 3 inferred relationships involving `run_pipeline()` (e.g. with `parse_redirections()` and `.new()`) actually correct?**
  _`run_pipeline()` has 3 INFERRED edges - model-reasoned connections that need verification._
- **Are the 4 inferred relationships involving `find_executable()` (e.g. with `run_builtin()` and `run_pipeline()`) actually correct?**
  _`find_executable()` has 4 INFERRED edges - model-reasoned connections that need verification._