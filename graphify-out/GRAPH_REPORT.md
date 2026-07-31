# Graph Report - shell-rust  (2026-07-31)

## Corpus Check
- 7 files · ~4,350 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 33 nodes · 62 edges · 6 communities detected
- Extraction: 58% EXTRACTED · 42% INFERRED · 0% AMBIGUOUS · INFERRED: 26 edges (avg confidence: 0.8)
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
2. `run_pipeline()` - 6 edges
3. `find_executable()` - 5 edges
4. `reap_background_jobs()` - 4 edges
5. `parse_redirections()` - 4 edges
6. `executables_starting_with()` - 4 edges
7. `main()` - 4 edges
8. `reap()` - 4 edges
9. `print_job()` - 3 edges
10. `open_redirect()` - 3 edges

## Surprising Connections (you probably didn't know these)
- `run_pipeline()` --calls--> `parse_redirections()`  [INFERRED]
  src/builtins.rs → src/redirection.rs
- `run_pipeline()` --calls--> `find_executable()`  [INFERRED]
  src/builtins.rs → src/path.rs
- `reap_background_jobs()` --calls--> `reap()`  [INFERRED]
  src/builtins.rs → src/jobs.rs
- `run_command()` --calls--> `home_dir()`  [INFERRED]
  src/builtins.rs → src/path.rs
- `run_command()` --calls--> `find_executable()`  [INFERRED]
  src/builtins.rs → src/path.rs

## Communities

### Community 0 - "Community 0"
Cohesion: 0.43
Nodes (4): longest_common_prefix(), ShellHelper, common_prefix(), completions()

### Community 1 - "Community 1"
Cohesion: 0.4
Nodes (5): is_builtin(), run_command(), emit(), parse_redirections(), Redirection

### Community 2 - "Community 2"
Cohesion: 0.33
Nodes (5): add_job(), Job, JobSnapshot, JobStatus, reap()

### Community 3 - "Community 3"
Cohesion: 0.6
Nodes (4): executables_starting_with(), find_executable(), home_dir(), is_executable()

### Community 4 - "Community 4"
Cohesion: 0.5
Nodes (3): print_job(), reap_background_jobs(), main()

### Community 5 - "Community 5"
Cohesion: 0.67
Nodes (2): open_redirect(), run_pipeline()

## Knowledge Gaps
- **4 isolated node(s):** `Redirection`, `JobStatus`, `JobSnapshot`, `Job`
  These have ≤1 connection - possible missing edges or undocumented components.
- **Thin community `Community 5`** (4 nodes): `job_marker()`, `open_redirect()`, `builtins.rs`, `run_pipeline()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `run_command()` connect `Community 1` to `Community 0`, `Community 2`, `Community 3`, `Community 4`, `Community 5`?**
  _High betweenness centrality (0.259) - this node is a cross-community bridge._
- **Why does `reap()` connect `Community 2` to `Community 0`, `Community 1`, `Community 4`?**
  _High betweenness centrality (0.134) - this node is a cross-community bridge._
- **Why does `add_job()` connect `Community 2` to `Community 0`, `Community 1`?**
  _High betweenness centrality (0.099) - this node is a cross-community bridge._
- **Are the 8 inferred relationships involving `run_command()` (e.g. with `parse_redirections()` and `.new()`) actually correct?**
  _`run_command()` has 8 INFERRED edges - model-reasoned connections that need verification._
- **Are the 3 inferred relationships involving `run_pipeline()` (e.g. with `parse_redirections()` and `find_executable()`) actually correct?**
  _`run_pipeline()` has 3 INFERRED edges - model-reasoned connections that need verification._
- **Are the 3 inferred relationships involving `find_executable()` (e.g. with `run_pipeline()` and `run_command()`) actually correct?**
  _`find_executable()` has 3 INFERRED edges - model-reasoned connections that need verification._
- **Are the 2 inferred relationships involving `reap_background_jobs()` (e.g. with `reap()` and `main()`) actually correct?**
  _`reap_background_jobs()` has 2 INFERRED edges - model-reasoned connections that need verification._