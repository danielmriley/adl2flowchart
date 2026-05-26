---
name: analyze-adl
description: Analyze an ADL (Analysis Description Language) file used in high-energy physics. Produces a structured markdown report covering parsed structure, objects, variables, regions, dependency graph, and disjointness analysis. Use when the user wants to analyze, review, or document an ADL file.
argument-hint: [path-to-adl-file]
allowed-tools: Read Write Bash Glob Grep
---

# ADL File Analysis

Read the ADL file at path `$ARGUMENTS` and produce a comprehensive analysis report as a markdown file.

## ADL Language Reference

ADL (Analysis Description Language) is a domain-specific language for describing high-energy physics event selection and analysis workflows. The key constructs are:

### Top-level blocks
- `info analysis` / `info adl`: Metadata block with fields like `title`, `experiment`, `id`, `sqrtS`, `lumi`, `arXiv`, `publication`, `doi`, `hepdata`, `adlauthor`, `datatier`
- `object <name>`: Defines a particle collection. Contains `take <base>` (inheritance), `select <cond>` (filter), `reject <cond>` (anti-filter), `define <var> = <expr>` (object-level variable)
- `define <var> = <expr>`: Top-level derived variable definition
- `region <name>`: Event selection region. Contains `select`, `reject`, `weight <name> <value>`, `trigger <cond>`, `histo`, `bin`
- `table <name>`: Efficiency/weight lookup table with `tabletype`, `nvars`, `errors`

### Union objects
- `take union(A, B)` or two separate `take` lines combine collections

### Operators
- Comparison: `>`, `<`, `>=`, `<=`, `==`, `!=`, `~=`
- Range inclusive: `[] low high` or `X [] low high` means low <= X <= high
- Range exclusive: `][ low high` or `X ][ low high` means X < low OR X > high (anti-range / veto window)
- Logical: `and`, `or`, `AND`, `OR`
- Arithmetic: `+`, `-`, `*`, `/`, `^`
- Ternary: `condition ? value1 : value2`
- Functions: `size()`, `sum()`, `abs()`, `sqrt()`, `dR()`, `dPhi()`, `dEta()`, `m()`, `pT()`, `eta()`, `phi()`, etc.

### Builtin objects
Standard predefined objects include: Electron, Muon, Tau, Photon, Jet, FatJet, MissingET, Track, Trk, etc. (case insensitive: `Ele`/`electron`/`Electron` all refer to the same base)

### Comments
Lines starting with `#` are comments. Commented-out blocks (e.g., `#region`, `#object`) represent inactive code that should be noted but not analyzed as active.

## Analysis Procedure

Perform each of the following steps. For each step, record whether it succeeded or had warnings.

### Step 1: Parse the ADL file

Read the file and extract:
- **Info block**: all metadata fields (title, experiment, id, sqrtS, lumi, publication, arXiv, doi, etc.)
- **Objects**: name, base type(s) from `take`, all `select`/`reject` criteria, any object-level `define` statements
- **Defines**: variable name, full expression, which objects/variables are referenced
- **Regions**: name, all statements in order (select, reject, weight, trigger, bin, histo), whether region inherits from another region
- **Tables**: name, type, dimensions
- **Commented-out blocks**: note any commented-out regions, objects, or defines

Count: objects, defines, regions (active vs commented-out), tables.

### Step 2: Build the dependency graph

Trace how each construct depends on others:
- Objects depend on their `take` base (builtin or user-defined object)
- Objects that use `union()` or multiple `take` lines depend on all constituent objects
- Defines depend on whatever objects/variables appear in their expressions
- Regions depend on whatever objects/variables/regions they reference in their statements
- Region inheritance (e.g., `baseline` appearing as a bare statement in another region) creates a dependency

Classify nodes as: Builtin, Object, Define, Region, Table.
Classify edges as: `take` (object inheritance) or `reference` (expression dependency).

Count total nodes and edges. Note any warnings (undefined references, circular dependencies, cross-file references).

### Step 3: Check object disjointness

For every pair of user-defined objects, determine if they can contain overlapping particles:
- **Disjoint**: Objects derived from different builtin particle types cannot overlap (e.g., muons vs jets)
- **Possibly overlapping**: Objects sharing a common ancestor may overlap. This includes:
  - Union objects that contain another object as a constituent
  - Objects that are subsets of other objects (same `take` chain with additional filters)
  - Objects derived from the same base with non-mutually-exclusive selections
- **Unknown**: If the relationship cannot be determined statically

For union objects, track all constituent base types for accurate disjointness reasoning.

### Step 4: Check region disjointness

For every pair of active regions, determine if they can select overlapping events:
- If only 0 or 1 active region exists, note that pairwise analysis is not applicable
- Regions with mutually exclusive requirements (e.g., `size(X) == 0` vs `size(X) >= 1`) are disjoint
- Regions inheriting from different baselines may or may not overlap depending on baseline criteria
- Regions with identical baselines but different additional cuts may overlap

## Output Format

Write the analysis to a markdown file at the same directory level as the input ADL file, named `<adl-filename-without-extension>_analysis.md` (e.g., `CMS-SUS-16-048_Delphes_analysis.md` for input `CMS-SUS-16-048_Delphes.adl`). Use the following structure exactly:

```markdown
# ADL Analysis: <filename without path>

**Analysis:** <title from info block, or "No title provided">

**Experiment:** <experiment> | **ID:** <id> | **Luminosity:** <lumi> fb^-1 | **sqrt(s):** <sqrtS> TeV
**Publication:** <publication> | **arXiv:** <arXiv>
**Date:** <today's date YYYY-MM-DD>

---

## Tool Results Summary

| Tool | Status |
|------|--------|
| `parse_adl_file` | <Success or Failed + reason> |
| `build_dependency_graph` | <Success or Failed + reason> |
| `check_disjoint_objects` | <Success or note> |
| `check_disjoint_regions` | <Success or note> |

---

## Parsed Structure

| Category | Count |
|----------|-------|
| Objects  | <N>   |
| Defines  | <N>   |
| Regions  | <N active> + <N commented out> |
| Tables   | <N>   |

### Objects

| Object | Base (take) | Selection Cuts |
|--------|-------------|----------------|
| **<name>** | `<base>` | <human-readable summary of cuts> |

<Notable observations about object definitions, e.g., union objects, subset relationships>

### Defined Variables

| Variable | Expression | Dependencies |
|----------|-----------|--------------|
| `<name>` | `<expression>` | <list of referenced objects/variables> |

### Region: <RegionName>

<Brief description of what this region implements, referencing the physics context if apparent from the info block or comments>

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | <select/reject/weight/trigger/bin> | <the ADL statement> | <brief physics interpretation> |

<Repeat for each active region>

### Commented-Out Regions

<List any commented-out regions with brief descriptions if determinable>

---

## Dependency Graph

**<N> nodes, <M> edges.** <Any warnings.>

### Node Breakdown

| Type | Count | Names |
|------|-------|-------|
| Builtin | <N> | <names> |
| Object | <N> | <names> |
| Define | <N> | <names> |
| Region | <N> | <names> |
| Table  | <N> | <names> |

### Dependency Hierarchy

<ASCII art showing the dependency tree from builtins at top through objects, defines, and regions. Use indentation, pipes, and backslashes to show relationships clearly.>

### Edge Types

| Kind | Count | Description |
|------|-------|-------------|
| `take` | <N> | Object inheritance |
| `reference` | <N> | Variable/expression dependencies |

---

## Object Disjointness Analysis

**<N> pairs checked:** <D> disjoint, <P> possibly overlapping, <U> unknown.

### Disjoint Pairs (<D>)

| Object A | Object B | Reason |
|----------|----------|--------|
| <name> | <name> | <reason, e.g., "Different particle types (muon vs jet)"> |

### Possibly Overlapping Pairs (<P>)

| Object A | Object B | Reason |
|----------|----------|--------|
| <name> | <name> | <reason, e.g., "`bjets` is a subset of `jets`"> |

---

## Region Disjointness Analysis

<Pairwise analysis or note that it is not applicable>

---

## Notes

<Bullet list of notable observations:>
<- Variables defined but not used in any active region>
<- Redundant or no-op cuts (e.g., `size(jets) >= 0`)>
<- Weights applied and their values>
<- Special operators used (anti-range `][`, ternary `?:`)>
<- Compound conditions parsed into separate cuts>
<- Any potential issues or ambiguities>
```

## Important Guidelines

- Parse the ADL file textually. Do NOT attempt to run the `smash` compiler.
- Be precise about operators: `[]` is inclusive range, `][` is exclusive/anti-range (veto window). Describe `][` as "outside" when summarizing cuts.
- When summarizing selection cuts for objects, use human-readable physics notation (e.g., "pT in [3.5, 30] GeV; |eta| < 2.4").
- For the dependency hierarchy ASCII art, show the actual structure of this specific analysis, not a generic template.
- For physics purpose in region cut tables, interpret based on the cut semantics (e.g., opposite-sign requirement, mass window, veto, etc.). If the purpose is not clear, describe the cut effect.
- If the info block is commented out (lines starting with `#info`), still extract the metadata from it.
- Count only uncommented `object`, `define`, `region`, and `table` blocks as active.
- When an object uses two `take` lines without `union()`, it is still a union object.
- When a region references another region name as a bare statement (not inside `select`), that is region inheritance.
- `bin` statements within a region define signal/control region bins -- count them and describe the binning scheme.
- Write the output file, then report completion to the user with a brief summary.
