#include "adl2/sema/sema.hpp"

#include <iostream>
#include <string>

using adl2::sema::ExtDecls;
using adl2::sema::Hir;
using adl2::sema::analyze_str;
using adl2::sema::object_table;

namespace {

int g_fails = 0;
int g_pass = 0;

void check(bool cond, const char* expr, const char* file, int line) {
  if (cond) {
    ++g_pass;
  } else {
    ++g_fails;
    std::cerr << "FAIL " << file << ":" << line << "  " << expr << "\n";
  }
}

#define CHECK(cond) check(static_cast<bool>(cond), #cond, __FILE__, __LINE__)

const char* kTiny =
    "info analysis\n"
    "  title \"cpp P0 tiny fixture\"\n"
    "\n"
    "define nEle = size(Ele)\n"
    "\n"
    "object goodEles\n"
    "  take Ele\n"
    "  select pT(Ele) > 20\n"
    "  select abs(eta(Ele)) < 2.1\n"
    "\n"
    "region SR\n"
    "  select nEle >= 1\n"
    "  select nEle >= 2 or MET.pt > 50\n";

void test_object_table_tiny() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kTiny, "tiny.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  std::string t = object_table(hir, false);
  CHECK(t.find("== objects ==") == 0);
  CHECK(t.find("goodEles") != std::string::npos);
  CHECK(t.find("<-") != std::string::npos);
  CHECK(t.find("exact") != std::string::npos);
  CHECK(t.find("subset of parent") != std::string::npos);
  CHECK(t.find("\x1b[") == std::string::npos);
}

void test_object_table_color() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kTiny, "tiny.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  std::string t = object_table(hir, true);
  CHECK(t.find("\x1b[1m") != std::string::npos);
}

}  // namespace

int main() {
  test_object_table_tiny();
  test_object_table_color();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
