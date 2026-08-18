#pragma once

#include <string>
#include <string_view>
#include <vector>

namespace adl2::rdgen {

enum class TermKind { Literal, Name, Group, Optional, Repeat };

struct Seq;

struct Term {
  TermKind kind = TermKind::Name;
  std::string text;
  std::vector<Seq> group;
};

struct Seq {
  std::vector<Term> terms;
};

struct Production {
  std::string name;
  std::vector<Seq> alts;
  int line = 1;
};

struct Grammar {
  std::vector<Production> prods;
  std::string error;
  int error_line = 0;
  int error_col = 0;

  const Production* find(std::string_view name) const;
};

Grammar parse_ebnf(std::string_view src);

std::string hyphen_to_underscore(std::string_view name);
std::string parse_method_name(std::string_view ebnf_name);
std::string format_seq(const Seq& seq);
std::string format_production(const Production& p);

enum class Shape {
  Alias,
  LeftAssoc,
  PrefixUnary,
  OptionalSuffix,
  Choice,
  KeywordSeq,
  TokenClass,
  Empty,
  Other,
};

const char* shape_name(Shape s);

struct ShapeInfo {
  Shape shape = Shape::Other;
  std::string next;
  std::vector<std::string> ops;
};

ShapeInfo classify(const Production& p);

/// True when the production is `keywords condition` (Literal or group of
/// Literals, then the name `condition`). Used to infer unmapped statement
/// words as Cut-shaped region statements (slice 2).
bool keyword_condition_kws(const Production& p, std::vector<std::string>& kws);

}  // namespace adl2::rdgen
