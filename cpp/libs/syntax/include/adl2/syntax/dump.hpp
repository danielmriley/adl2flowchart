#pragma once

#include "adl2/syntax/ast.hpp"

#include <string>
#include <string_view>

namespace adl2::syntax {

/// Quote a string the way Rust `Debug` formats `&str` / `String`
/// (`"{:?}"`) — required for dump parity with `adl_syntax::dump_ast`.
std::string rust_debug_str(std::string_view s);

/// Canonical AST dump matching Rust `adl_syntax::dump_ast` (SPEC_ARCHITECTURE §3).
/// `src` is retained for API parity with Rust; spans use line/col stored on nodes.
std::string dump_ast(std::string_view src, const FileAst& file);

}  // namespace adl2::syntax
