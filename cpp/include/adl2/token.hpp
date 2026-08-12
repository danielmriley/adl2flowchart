#pragma once

#include "adl2/span.hpp"

#include <string>

namespace adl2 {

enum class TokKind {
  Eof,
  Newline,
  Ident,
  Int,
  Real,
  String,
  PathLike,  // bare weight-file style token (arg context only)

  // Keywords (case-insensitive). Keep in sync with SPEC_LANGUAGE §2.
  KwInfo,
  KwDefine,
  KwDef,
  KwObject,
  KwObj,
  KwComposite,
  KwTrigger,
  KwTake,
  KwUsing,
  KwSelect,
  KwCut,
  KwCmd,
  KwCommand,
  KwReject,
  KwRegion,
  KwAlgo,
  KwHistoList,
  KwBin,
  KwHisto,
  KwWeight,
  KwTable,
  KwTabletype,
  KwNvars,
  KwErrors,
  KwUnion,
  KwProcess,
  KwCounts,
  KwCountsformat,
  KwPrint,
  KwSave,
  KwSort,
  KwAll,
  KwNone,
  KwAnd,
  KwOr,
  KwNot,
  KwTrue,
  KwFalse,

  // Operators / punctuation
  Plus,
  Minus,
  Star,
  Slash,
  Caret,
  Assign,   // =
  Colon,
  Question,
  Bang,     // !
  Dot,
  Comma,
  Underscore,
  LParen,
  RParen,
  LBracket,
  RBracket,
  LBrace,
  RBrace,
  Pipe,
  Gt,
  Lt,
  Ge,
  Le,
  EqEq,
  Ne,
  TildeEq,  // ~=
  AndAnd,   // &&
  OrOr,     // ||
  BandIncl, // []
  BandExcl, // ][
  Arrow,    // ->
  PlusMinus, // +-
};

struct Token {
  TokKind kind = TokKind::Eof;
  std::string text;
  Span span;
};

const char* tok_kind_name(TokKind k);

}  // namespace adl2
