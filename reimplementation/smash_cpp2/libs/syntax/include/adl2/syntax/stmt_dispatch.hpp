// Shared keyword tables. Adding a section or region-body keyword is one
// row here plus parse_* , grammar.ebnf, and method_map.txt. Do not add a
// third if-chain in at_section_start / parse_section / at_stmt_keyword /
// parse_region_stmt / recovery.
#pragma once

#include "adl2/syntax/token.hpp"

#include <cstddef>

namespace adl2::syntax {

enum class SectionHook {
  Info,
  Define,
  Object,
  Region,
  Table,
  Countsformat,
};

struct SectionRow {
  TokKind kind;
  const char* keyword;
  SectionHook hook;
};

inline constexpr SectionRow kSectionTable[] = {
    {TokKind::KwInfo, "info", SectionHook::Info},
    {TokKind::KwDefine, "define", SectionHook::Define},
    {TokKind::KwDef, "def", SectionHook::Define},
    {TokKind::KwObject, "object", SectionHook::Object},
    {TokKind::KwObj, "obj", SectionHook::Object},
    {TokKind::KwComposite, "composite", SectionHook::Object},
    {TokKind::KwTrigger, "trigger", SectionHook::Object},
    {TokKind::KwRegion, "region", SectionHook::Region},
    {TokKind::KwAlgo, "algo", SectionHook::Region},
    {TokKind::KwHistoList, "histoList", SectionHook::Region},
    {TokKind::KwTable, "table", SectionHook::Table},
    {TokKind::KwCountsformat, "countsformat", SectionHook::Countsformat},
};

inline constexpr std::size_t kSectionTableSize =
    sizeof(kSectionTable) / sizeof(kSectionTable[0]);

inline const SectionRow* find_section(TokKind kind) {
  for (std::size_t i = 0; i < kSectionTableSize; ++i) {
    if (kSectionTable[i].kind == kind) return &kSectionTable[i];
  }
  return nullptr;
}

enum class RegionStmtHook {
  Cut,        // select | cut | cmd | command
  Reject,
  Bin,        // bin  (contextual `bins` is a separate hook)
  Weight,
  Trigger,
  Histo,
  Save,
  Counts,
  Print,
  Sort,
  TakeUsing,  // take | using → region-ref
};

struct StmtRow {
  TokKind kind;
  const char* keyword;
  RegionStmtHook hook;
};

inline constexpr StmtRow kRegionStmtTable[] = {
    {TokKind::KwSelect, "select", RegionStmtHook::Cut},
    {TokKind::KwCut, "cut", RegionStmtHook::Cut},
    {TokKind::KwCmd, "cmd", RegionStmtHook::Cut},
    {TokKind::KwCommand, "command", RegionStmtHook::Cut},
    {TokKind::KwReject, "reject", RegionStmtHook::Reject},
    {TokKind::KwBin, "bin", RegionStmtHook::Bin},
    {TokKind::KwWeight, "weight", RegionStmtHook::Weight},
    {TokKind::KwTrigger, "trigger", RegionStmtHook::Trigger},
    {TokKind::KwHisto, "histo", RegionStmtHook::Histo},
    {TokKind::KwSave, "save", RegionStmtHook::Save},
    {TokKind::KwCounts, "counts", RegionStmtHook::Counts},
    {TokKind::KwPrint, "print", RegionStmtHook::Print},
    {TokKind::KwSort, "sort", RegionStmtHook::Sort},
    {TokKind::KwTake, "take", RegionStmtHook::TakeUsing},
    {TokKind::KwUsing, "using", RegionStmtHook::TakeUsing},
};

inline constexpr std::size_t kRegionStmtTableSize =
    sizeof(kRegionStmtTable) / sizeof(kRegionStmtTable[0]);

inline const StmtRow* find_region_stmt(TokKind kind) {
  for (std::size_t i = 0; i < kRegionStmtTableSize; ++i) {
    if (kRegionStmtTable[i].kind == kind) return &kRegionStmtTable[i];
  }
  return nullptr;
}

inline bool is_region_stmt_keyword(TokKind kind) {
  return find_region_stmt(kind) != nullptr;
}

}  // namespace adl2::syntax
