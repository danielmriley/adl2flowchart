#include "constraint_encoder.hpp"

#include "driver.h"
#include "semantic_checks.h"

#include <algorithm>
#include <cctype>
#include <cmath>
#include <fstream>
#include <functional>
#include <set>
#include <sstream>

namespace adl {

// ======================================================================
// Canonical key synthesis
// ======================================================================

bool isTagPropertyName(const std::string& name) {
  std::string lower = tolower(name);
  return lower.find("btag") != std::string::npos ||
         lower.find("ctag") != std::string::npos ||
         lower.find("tautag") != std::string::npos ||
         lower.find("charge") != std::string::npos;
}

std::string buildKeyFromVar(VarNode* vn) {
  if (!vn) return "";
  std::string id = vn->getId();
  std::string dot = vn->getDotOp();
  std::string alias = vn->getAlias();

  std::string prop = dot;
  if (prop.empty() && !alias.empty()) {
    // braced syntax can land the property in the alias slot
    size_t lastDot = alias.find_last_of('.');
    prop = (lastDot != std::string::npos) ? alias.substr(lastDot + 1) : alias;
    if (!isTagPropertyName(prop)) prop.clear();
  }

  // Render as object[index].property so the index stays attached to the
  // object, never to the property.
  std::string key = id;
  std::vector<int> acc = vn->getAccessor();
  if (!acc.empty()) {
    key += "[";
    for (size_t i = 0; i < acc.size(); ++i) {
      if (i > 0) key += ":";
      if (acc[i] != 6213) key += std::to_string(acc[i]);
    }
    key += "]";
  }
  if (!prop.empty()) key += "." + prop;
  return key;
}

namespace {
std::map<std::string, std::string> g_takeAliasToCanon;
std::map<std::string, std::string> g_pureAlias;
bool g_takeAliasesReady = false;

void registerTakeAlias(const std::string& canon, const std::string& alias) {
  std::string c = toupper(canon);
  std::string a = toupper(alias);
  g_takeAliasToCanon[a] = c;
  g_takeAliasToCanon[c] = c;
}

// object X / take Y, with no cuts and a single take, is just a rename of Y.
// Filtered collections (any select/reject) are NOT aliases: their indexed
// elements are different event quantities than the parent's.
void buildPureAliases(Driver& drv) {
  g_pureAlias.clear();
  std::map<std::string, std::string> direct;
  for (auto& n : drv.ast) {
    if (n->getToken() != "OBJECT") continue;
    astObjectNode* on = getObjectNode(n);
    int takes = 0;
    int cuts = 0;
    std::string src;
    for (auto& s : on->getStatements()) {
      std::string t = s->getToken();
      if (t == "TAKE") {
        takes++;
        Expr* cond = getCommandNode(s)->getCondition();
        if (cond && (cond->getToken() == "ID" || cond->getToken() == "VAR"))
          src = cond->getId();
      } else if (t == "SELECT" || t == "REJECT" || t == "CUT" || t == "CMD" ||
                 t == "COMMAND") {
        cuts++;
      }
    }
    if (takes == 1 && cuts == 0 && !src.empty())
      direct[toupper(on->getId())] = toupper(src);
  }
  for (const auto& kv : direct) {
    std::string cur = kv.second;
    std::set<std::string> seen{kv.first};
    while (direct.count(cur) && !seen.count(cur)) {
      seen.insert(cur);
      cur = direct[cur];
    }
    g_pureAlias[kv.first] = cur;
  }
}
}  // namespace

void resetTakeAliasCache() {
  g_takeAliasesReady = false;
  g_takeAliasToCanon.clear();
  g_pureAlias.clear();
}

void ensureTakeAliases(Driver& drv) {
  if (g_takeAliasesReady) return;
  // Spelling variants of base collections and the MET singleton family.
  registerTakeAlias("MUON", "MUO");
  registerTakeAlias("ELECTRON", "ELE");
  registerTakeAlias("MET", "MISSINGET");
  registerTakeAlias("MET", "METLV");

  std::string path = drv.getLibPath().string() + "/object_aliases.txt";
  std::ifstream fin(path);
  if (fin.good()) {
    std::string line;
    while (std::getline(fin, line)) {
      if (line.empty() || line[0] == '#') continue;
      std::stringstream ss(line);
      std::string canon, alias;
      if (!(ss >> canon)) continue;
      registerTakeAlias(canon, canon);
      while (ss >> alias) registerTakeAlias(canon, alias);
    }
  }
  buildPureAliases(drv);
  g_takeAliasesReady = true;
}

std::string canonicalTakeRoot(const std::string& raw, Driver& drv) {
  ensureTakeAliases(drv);
  std::string u = toupper(raw);
  auto pit = g_pureAlias.find(u);
  if (pit != g_pureAlias.end()) u = pit->second;
  auto it = g_takeAliasToCanon.find(u);
  if (it != g_takeAliasToCanon.end()) return it->second;
  return u;
}

std::string objectFromConstraintKey(const std::string& key) {
  if (key.rfind("size(", 0) == 0 && key.size() > 6 && key.back() == ')') {
    return key.substr(5, key.size() - 6);
  }
  size_t dot = key.find('.');
  size_t bracket = key.find('[');
  size_t end = key.size();
  if (dot != std::string::npos) end = std::min(end, dot);
  if (bracket != std::string::npos) end = std::min(end, bracket);
  return key.substr(0, end);
}

std::string bracketIndexSuffix(const std::string& key) {
  size_t b = key.find('[');
  if (b == std::string::npos) return "";
  size_t e = key.find(']', b);
  if (e == std::string::npos) return key.substr(b);
  return key.substr(b, e - b + 1);
}

std::string canonicalConstraintKey(const std::string& key, Driver& drv) {
  if (key.rfind("size(", 0) == 0 && key.size() > 6 && key.back() == ')') {
    std::string inner = key.substr(5, key.size() - 6);
    return "size(" + canonicalTakeRoot(inner, drv) + ")";
  }
  std::string lowkey = tolower(key);
  if (lowkey.rfind("dphi(", 0) == 0 || lowkey.rfind("dr(", 0) == 0 ||
      lowkey.rfind("deta(", 0) == 0) {
    return toupper(key);
  }
  std::string obj = objectFromConstraintKey(key);
  if (obj.empty()) return key;
  size_t dot = key.find('.');
  size_t bracket = key.find('[');
  std::string canonObj = canonicalTakeRoot(obj, drv);
  // A bare MET-family name used as a value means its magnitude: unify
  // "MET" with "MET.pt" so selects and bins land on the same variable.
  if (dot == std::string::npos && bracket == std::string::npos &&
      canonObj == "MET")
    return "MET.pt";
  std::string suffix;
  if (bracket != std::string::npos && (dot == std::string::npos || bracket < dot))
    suffix = key.substr(bracket);
  else if (dot != std::string::npos)
    suffix = key.substr(dot);
  // Properties compare case-insensitively: pT == pt == PT.
  size_t lastDot = suffix.find_last_of('.');
  if (lastDot != std::string::npos) {
    for (size_t i = lastDot + 1; i < suffix.size(); ++i)
      suffix[i] = static_cast<char>(std::tolower(
          static_cast<unsigned char>(suffix[i])));
  }
  return canonObj + suffix;
}

bool keyUsesIntSort(const std::string& key) {
  return key.rfind("size(", 0) == 0;
}

// ======================================================================
// Encoder
// ======================================================================

namespace {

struct EncodeCtx {
  Driver* drv = nullptr;
  std::map<std::string, Expr*> defineBodies;
  std::map<std::string, RegionNode*> regionByName;
  std::set<std::string> activeDefines;  // cycle guard
};

bool isAndOp(const std::string& op) {
  return op == "AND" || op == "and" || op == "&&";
}

bool isOrOp(const std::string& op) {
  return op == "OR" || op == "or" || op == "||";
}

bool isAllKeyword(Expr* e) {
  if (!e) return false;
  if (e->getToken() == "ID" || e->getToken() == "VAR")
    return toupper(e->getId()) == "ALL";
  if (e->getToken() == "FUNCTION")
    return toupper(getFunctionNode(e)->getId()) == "ALL";
  return false;
}

bool numericValueOf(Expr* e, double& out) {
  if (!e) return false;
  std::string t = e->getToken();
  if (t == "INT" || t == "REAL") {
    out = e->value();
    return true;
  }
  if (t == "EXPROP" || t == "FACTOROP") {
    double v = e->value();
    if (!std::isnan(v)) {
      out = v;
      return true;
    }
  }
  return false;
}

std::string fmtNum(double v) {
  std::ostringstream ss;
  ss << v;
  return ss.str();
}

// Per-object kinematic properties: a cut on prop(collection) without an
// index has ambiguous quantifier semantics and must not be scalarized.
bool isPerObjectProperty(const std::string& lowname) {
  static const std::set<std::string> props = {
      "pt",  "eta", "phi",  "m",      "mass", "e",
      "rap", "energy", "abseta", "px", "py", "pz"};
  return props.count(lowname) > 0 || isTagPropertyName(lowname);
}

bool isAngularSep(const std::string& lowname) {
  return lowname == "dphi" || lowname == "dr" || lowname == "deta";
}

// Event-level scalar functions of whole collections.
bool isEventScalarFunction(const std::string& lowname) {
  static const std::set<std::string> fns = {
      "ht",  "met", "meff", "aplanarity", "sphericity", "fht", "fmet",
      "mht", "metsig", "sum"};
  return fns.count(lowname) > 0;
}

// Objects that carry one value per event rather than a collection.
bool isSingletonRoot(const std::string& canonRoot) {
  static const std::set<std::string> roots = {
      "MET", "MISSINGET", "METLV", "MHT", "HT", "SCALARHT",
      "DELPHES_SCALARHT", "METSIG"};
  return roots.count(canonRoot) > 0;
}

// Key synthesis for an expression appearing on the variable side of a
// comparison. Empty string means "cannot name this quantity".
std::string keyFromExpr(Expr* e, EncodeCtx& ctx);

std::string keyFromFunction(FunctionNode* fn, EncodeCtx& ctx) {
  std::string lowname = tolower(fn->getId());
  auto params = fn->getParams();

  if (lowname == "size" && params.size() == 1) {
    Expr* p = params[0];
    if (p->getToken() == "ID" || p->getToken() == "VAR")
      return "size(" + canonicalTakeRoot(getVarNode(p)->getId(), *ctx.drv) + ")";
    return "";
  }

  if (isAngularSep(lowname) && params.size() >= 2) {
    std::string a = keyFromExpr(params[0], ctx);
    std::string b = keyFromExpr(params[1], ctx);
    if (a.empty() || b.empty()) return "";
    if (a > b) std::swap(a, b);
    return lowname + "(" + a + "," + b + ")";
  }

  if (isPerObjectProperty(lowname) && !params.empty()) {
    std::string objKey = keyFromExpr(params[0], ctx);
    if (objKey.empty()) return "";
    return canonicalConstraintKey(objKey + "." + lowname, *ctx.drv);
  }

  if (lowname == "abs" && params.size() == 1) {
    std::string inner = keyFromExpr(params[0], ctx);
    if (inner.empty()) return "";
    return "abs(" + inner + ")";
  }

  // Generic function of nameable arguments: opaque but consistent scalar.
  std::ostringstream key;
  key << lowname << "(";
  for (size_t i = 0; i < params.size(); ++i) {
    double num;
    std::string argKey;
    if (numericValueOf(params[i], num)) {
      argKey = fmtNum(num);
    } else {
      argKey = keyFromExpr(params[i], ctx);
      if (argKey.empty()) return "";
    }
    if (i) key << ",";
    key << argKey;
  }
  key << ")";
  return key.str();
}

std::string keyFromExpr(Expr* e, EncodeCtx& ctx) {
  if (!e) return "";
  std::string t = e->getToken();
  if (t == "ID" || t == "VAR") {
    std::string raw = buildKeyFromVar(getVarNode(e));
    if (raw.empty()) return "";
    return canonicalConstraintKey(raw, *ctx.drv);
  }
  if (t == "FUNCTION") return keyFromFunction(getFunctionNode(e), ctx);
  return "";
}

// Quantifier classification: can this key be modeled as one scalar per
// event? Collection properties without an index cannot.
bool keyIsEventScalar(const std::string& key, EncodeCtx& ctx, bool topLevel);

bool argListEventScalar(const std::string& args, EncodeCtx& ctx) {
  int depth = 0;
  std::string cur;
  for (char c : args) {
    if (c == '(') depth++;
    if (c == ')') depth--;
    if (c == ',' && depth == 0) {
      if (!cur.empty() && !keyIsEventScalar(cur, ctx, false)) return false;
      cur.clear();
      continue;
    }
    cur += c;
  }
  if (!cur.empty() && !keyIsEventScalar(cur, ctx, false)) return false;
  return true;
}

bool keyIsEventScalar(const std::string& key, EncodeCtx& ctx, bool topLevel) {
  if (key.empty()) return false;
  if (!key.empty() && (std::isdigit(static_cast<unsigned char>(key[0])) ||
                       key[0] == '-'))
    return true;  // numeric literal argument
  if (key.rfind("size(", 0) == 0) return true;

  size_t par = key.find('(');
  if (par != std::string::npos && key.back() == ')') {
    std::string fname = tolower(key.substr(0, par));
    if (isEventScalarFunction(fname)) return true;
    // Any other function key is scalar iff every argument is scalar
    // (indexed object, MET-family object, numeric literal, ...).
    std::string args = key.substr(par + 1, key.size() - par - 2);
    return argListEventScalar(args, ctx);
  }

  if (key.find('[') != std::string::npos) return true;  // indexed element

  size_t dot = key.find('.');
  std::string root = objectFromConstraintKey(key);
  std::string canonRoot = canonicalTakeRoot(root, *ctx.drv);
  if (dot != std::string::npos) {
    std::string prop = tolower(key.substr(key.find_last_of('.') + 1));
    if (isSingletonRoot(canonRoot)) return true;
    if (isEventScalarFunction(prop)) return true;
    return false;  // collection property without index
  }

  // Bare name. Defines are per-event scalars (they shadow external object
  // names like HT); objects are collections unless singleton-family.
  if (ctx.drv->checkDefinitionTable(root) == 0) return true;
  if (isSingletonRoot(canonRoot)) return true;
  if (ctx.drv->checkObjectTable(root) == 0 ||
      ctx.drv->check_object_table(root) == 0) {
    return false;
  }
  (void)topLevel;
  return true;
}

rf::Formula encodeExpr(Expr* e, EncodeCtx& ctx);

// Linear expression over event-scalar keys: sum(coeff*key) + constant.
struct LinExpr {
  std::vector<rf::Term> terms;
  double c = 0.0;

  void add(const LinExpr& o, double scale) {
    c += scale * o.c;
    for (const auto& t : o.terms) {
      bool merged = false;
      for (auto& mt : terms) {
        if (mt.key == t.key) {
          mt.coeff += scale * t.coeff;
          merged = true;
          break;
        }
      }
      if (!merged) terms.push_back(rf::Term{scale * t.coeff, t.key});
    }
    terms.erase(std::remove_if(terms.begin(), terms.end(),
                               [](const rf::Term& t) { return t.coeff == 0.0; }),
                terms.end());
  }
};

bool linearize(Expr* e, EncodeCtx& ctx, LinExpr& out) {
  if (!e) return false;
  double num;
  if (numericValueOf(e, num)) {
    out.c += num;
    return true;
  }
  std::string t = e->getToken();
  if (t == "ID" || t == "VAR" || t == "FUNCTION") {
    std::string key = keyFromExpr(e, ctx);
    if (key.empty() || !keyIsEventScalar(key, ctx, false)) return false;
    LinExpr one;
    one.terms.push_back(rf::Term{1.0, key});
    out.add(one, 1.0);
    return true;
  }
  if (t == "EXPROP" || t == "FACTOROP") {
    BinNode* bn = getBinNode(e);
    std::string op = bn->getOp();
    if (op == "+" || op == "-") {
      LinExpr l, r;
      if (!linearize(bn->getLHS(), ctx, l) || !linearize(bn->getRHS(), ctx, r))
        return false;
      out.add(l, 1.0);
      out.add(r, op == "+" ? 1.0 : -1.0);
      return true;
    }
    if (op == "*") {
      double k;
      Expr* other = nullptr;
      if (numericValueOf(bn->getLHS(), k)) other = bn->getRHS();
      else if (numericValueOf(bn->getRHS(), k)) other = bn->getLHS();
      if (!other) return false;  // nonlinear product
      LinExpr o;
      if (!linearize(other, ctx, o)) return false;
      out.add(o, k);
      return true;
    }
    if (op == "/" || op == "div") {
      double k;
      if (!numericValueOf(bn->getRHS(), k) || k == 0.0) return false;
      LinExpr l;
      if (!linearize(bn->getLHS(), ctx, l)) return false;
      out.add(l, 1.0 / k);
      return true;
    }
  }
  return false;
}

rf::CmpOp cmpFromString(const std::string& cop, bool& ok) {
  ok = true;
  if (cop == "<") return rf::CmpOp::LT;
  if (cop == "<=") return rf::CmpOp::LE;
  if (cop == ">") return rf::CmpOp::GT;
  if (cop == ">=") return rf::CmpOp::GE;
  if (cop == "==") return rf::CmpOp::EQ;
  if (cop == "!=" || cop == "~=") return rf::CmpOp::NE;
  ok = false;
  return rf::CmpOp::EQ;
}

rf::CmpOp flipCmp(rf::CmpOp op) {
  switch (op) {
    case rf::CmpOp::LT: return rf::CmpOp::GT;
    case rf::CmpOp::GT: return rf::CmpOp::LT;
    case rf::CmpOp::LE: return rf::CmpOp::GE;
    case rf::CmpOp::GE: return rf::CmpOp::LE;
    default: return op;
  }
}

// terms op value, simplifying a fully-constant comparison to True/False.
rf::Formula atomOrConst(LinExpr lin, rf::CmpOp op, double rhsConst) {
  double value = rhsConst - lin.c;
  if (lin.terms.empty()) {
    bool truth = false;
    switch (op) {
      case rf::CmpOp::LT: truth = 0.0 < value; break;
      case rf::CmpOp::LE: truth = 0.0 <= value; break;
      case rf::CmpOp::GT: truth = 0.0 > value; break;
      case rf::CmpOp::GE: truth = 0.0 >= value; break;
      case rf::CmpOp::EQ: truth = 0.0 == value; break;
      case rf::CmpOp::NE: truth = 0.0 != value; break;
    }
    return truth ? rf::fTrue() : rf::fFalse();
  }
  return rf::fLinearAtom(std::move(lin.terms), op, value);
}

// Exact encoding of (L / D) op c with a non-constant denominator:
//   (D > 0 ∧ L op c·D) ∨ (D < 0 ∧ L flip(op) c·D)
// D == 0 (undefined ratio) is treated as failing the cut.
rf::Formula encodeRatioCompare(Expr* ratioSide, rf::CmpOp op, double c,
                               EncodeCtx& ctx) {
  BinNode* bn = getBinNode(ratioSide);
  LinExpr L, D;
  if (!linearize(bn->getLHS(), ctx, L) || !linearize(bn->getRHS(), ctx, D))
    return rf::fUnknown("ratio with un-encodable numerator or denominator");
  if (D.terms.empty()) return rf::fUnknown("constant denominator ratio");

  auto branch = [&](bool positive) {
    LinExpr dCopy = D;
    rf::Formula sign =
        atomOrConst(dCopy, positive ? rf::CmpOp::GT : rf::CmpOp::LT, 0.0);
    LinExpr diff = L;          // L - c*D  op  0
    diff.add(D, -c);
    rf::Formula rel = atomOrConst(diff, positive ? op : flipCmp(op), 0.0);
    return rf::fAnd({sign, rel});
  };
  return rf::fOr({branch(true), branch(false)});
}

// Bounded expansion of a cut on a collection property without an index,
// e.g. "pT(jets) > 100". The intended quantifier (any/all elements) is
// ambiguous, so each projection must be sound under BOTH readings:
//   plus  = (∃ i<k: size>i ∧ cut(C[i]))  ∨  size > k
//           — any event passing under either reading lands here
//   minus = (∀ i<k: size>i ⇒ cut(C[i])) ∧ 1 <= size <= k
//           — with all elements covered and nonempty, ∀ holds and ∀⇒∃
constexpr int kQuantBound = 3;

rf::Formula tryBoundedCollectionCut(const std::string& key, rf::CmpOp op,
                                    double value, EncodeCtx& ctx) {
  if (key.find('(') != std::string::npos ||
      key.find('[') != std::string::npos)
    return rf::fUnknown("");
  size_t dot = key.find('.');
  if (dot == std::string::npos || key.find('.', dot + 1) != std::string::npos)
    return rf::fUnknown("");
  std::string root = key.substr(0, dot);
  std::string prop = key.substr(dot + 1);
  std::string sizeKey = "size(" + root + ")";

  std::vector<rf::Formula> existAlts;
  std::vector<rf::Formula> forallParts;
  for (int i = 0; i < kQuantBound; ++i) {
    std::string elemKey = root + "[" + std::to_string(i) + "]." + prop;
    rf::Formula present = rf::fAtom(sizeKey, rf::CmpOp::GE, i + 1);
    rf::Formula absent = rf::fAtom(sizeKey, rf::CmpOp::LE, i);
    rf::Formula cut = rf::fAtom(elemKey, op, value);
    existAlts.push_back(rf::fAnd({present, cut}));
    forallParts.push_back(rf::fOr({absent, cut}));
  }
  existAlts.push_back(rf::fAtom(sizeKey, rf::CmpOp::GT, kQuantBound));
  forallParts.push_back(rf::fAtom(sizeKey, rf::CmpOp::GE, 1));
  forallParts.push_back(rf::fAtom(sizeKey, rf::CmpOp::LE, kQuantBound));

  return rf::fDual(rf::fOr(std::move(existAlts)),
                   rf::fAnd(std::move(forallParts)),
                   "bounded any/all expansion (k=" +
                       std::to_string(kQuantBound) + ") for collection cut: " +
                       key);
}

rf::Formula leafFromCompare(BinNode* bn, EncodeCtx& ctx) {
  std::string op = bn->getOp();
  Expr* lhs = bn->getLHS();
  Expr* rhs = bn->getRHS();

  bool opOk = false;
  rf::CmpOp fop = cmpFromString(op, opOk);
  if (!opOk) return rf::fUnknown("unsupported comparison operator " + op);

  // ratio pattern: (L/D) op const  or  const op (L/D)
  auto isRatio = [&](Expr* e) {
    if (!e || e->getToken() != "FACTOROP") return false;
    BinNode* b = getBinNode(e);
    if (b->getOp() != "/" && b->getOp() != "div") return false;
    double k;
    return !numericValueOf(b->getRHS(), k);  // non-constant denominator
  };
  double c;
  if (isRatio(lhs) && numericValueOf(rhs, c))
    return encodeRatioCompare(lhs, fop, c, ctx);
  if (isRatio(rhs) && numericValueOf(lhs, c))
    return encodeRatioCompare(rhs, flipCmp(fop), c, ctx);

  LinExpr l, r;
  if (linearize(lhs, ctx, l) && linearize(rhs, ctx, r)) {
    LinExpr diff = l;
    diff.add(r, -1.0);
    double rhsConst = -diff.c;
    diff.c = 0.0;
    return atomOrConst(std::move(diff), fop, rhsConst);
  }

  // Collection cut without an index: bounded quantifier expansion.
  double num = 0.0;
  Expr* varSide = nullptr;
  if (numericValueOf(rhs, num)) varSide = lhs;
  else if (numericValueOf(lhs, num)) varSide = rhs;
  if (varSide) {
    std::string key = keyFromExpr(varSide, ctx);
    if (!key.empty() && !keyIsEventScalar(key, ctx, true)) {
      rf::CmpOp o = (varSide == rhs) ? flipCmp(fop) : fop;
      rf::Formula d = tryBoundedCollectionCut(key, o, num, ctx);
      if (d.kind == rf::FKind::Dual) return d;
      return rf::fUnknown("collection-level cut without index: " + key);
    }
  }
  return rf::fUnknown("cannot encode comparison (" + op + ")");
}

rf::Formula encodeExpr(Expr* e, EncodeCtx& ctx) {
  if (!e) return rf::fTrue();
  std::string t = e->getToken();

  if (t == "LOGICOP") {
    BinNode* bn = getBinNode(e);
    std::string op = bn->getOp();
    if (isAndOp(op))
      return rf::fAnd({encodeExpr(bn->getLHS(), ctx), encodeExpr(bn->getRHS(), ctx)});
    if (isOrOp(op))
      return rf::fOr({encodeExpr(bn->getLHS(), ctx), encodeExpr(bn->getRHS(), ctx)});
    return rf::fUnknown("logic operator " + op);
  }

  if (t == "COMPAREOP") {
    return leafFromCompare(getBinNode(e), ctx);
  }

  if (t == "ITE") {
    ITENode* ite = getITENode(e);
    rf::Formula g = encodeExpr(ite->getCondition(), ctx);
    rf::Formula thenF = isAllKeyword(ite->getThenBranch())
                            ? rf::fTrue()
                            : encodeExpr(ite->getThenBranch(), ctx);
    rf::Formula elseF = rf::fTrue();
    if (ite->getElseBranch() && !isAllKeyword(ite->getElseBranch()))
      elseF = encodeExpr(ite->getElseBranch(), ctx);
    // exact: (g AND then) OR (NOT g AND else)
    return rf::fOr({rf::fAnd({g, thenF}), rf::fAnd({rf::fNot(g), elseF})});
  }

  if (t == "FUNCTION") {
    FunctionNode* fn = getFunctionNode(e);
    std::string lowname = tolower(fn->getId());
    if (lowname == "not" && fn->getParams().size() == 1)
      return rf::fNot(encodeExpr(fn->getParams()[0], ctx));
    if (toupper(fn->getId()) == "ALL") return rf::fTrue();
    return rf::fUnknown("bare function call '" + fn->getId() + "' as condition");
  }

  if (t == "ID" || t == "VAR") {
    std::string id = e->getId();
    if (toupper(id) == "ALL") return rf::fTrue();
    auto dit = ctx.defineBodies.find(id);
    if (dit != ctx.defineBodies.end()) {
      if (ctx.activeDefines.count(id))
        return rf::fUnknown("cyclic define '" + id + "'");
      ctx.activeDefines.insert(id);
      rf::Formula f = encodeExpr(dit->second, ctx);
      ctx.activeDefines.erase(id);
      return f;
    }
    return rf::fUnknown("bare identifier '" + id + "' as condition");
  }

  return rf::fUnknown("unsupported construct (" + t + ")");
}

}  // namespace

int buildRegionFormulas(Driver& drv, std::vector<RegionFormulaInfo>& out) {
  out.clear();
  ensureTakeAliases(drv);

  EncodeCtx ctx;
  ctx.drv = &drv;
  for (auto& n : drv.ast) {
    if (n->getToken() == "DEFINE") {
      DefineNode* dn = getDefineNode(n);
      ctx.defineBodies[dn->getId()] = dn->getBody();
    } else if (n->getToken() == "REGION") {
      RegionNode* rn = getRegionNode(n);
      ctx.regionByName[rn->getId()] = rn;
    }
  }

  // Region formulas with inheritance: a select naming another region
  // inlines that region's full formula.
  std::map<std::string, rf::Formula> memo;
  std::set<std::string> active;

  std::function<rf::Formula(RegionNode*, RegionFormulaInfo*)> encodeRegion =
      [&](RegionNode* rn, RegionFormulaInfo* info) -> rf::Formula {
    auto mit = memo.find(rn->getId());
    if (mit != memo.end() && !info) return mit->second;
    if (active.count(rn->getId()))
      return rf::fUnknown("cyclic region reference '" + rn->getId() + "'");
    active.insert(rn->getId());

    std::vector<rf::Formula> conj;
    for (auto& stmt : rn->getStatements()) {
      std::string stok = stmt->getToken();

      if (stok == "SELECT" || stok == "REJECT" || stok == "CUT" ||
          stok == "CMD" || stok == "COMMAND") {
        CommandNode* cn = getCommandNode(stmt);
        Expr* cond = cn->getCondition();
        bool isReject = (stok == "REJECT");

        if (cond && (cond->getToken() == "ID" || cond->getToken() == "VAR")) {
          auto rit = ctx.regionByName.find(cond->getId());
          if (rit != ctx.regionByName.end() && rit->second != rn) {
            if (info) info->inherits.push_back(cond->getId());
            rf::Formula inherited = encodeRegion(rit->second, nullptr);
            conj.push_back(isReject ? rf::fNot(inherited) : inherited);
            continue;
          }
        }

        if (info) info->selectStmts++;
        rf::Formula f = encodeExpr(cond, ctx);
        if (isReject) f = rf::fNot(f);
        if (info && !rf::hasUnknown(f)) info->selectStmtsExact++;
        conj.push_back(std::move(f));
      } else if (stok == "BIN") {
        // Bins partition a region; they do not constrain membership, but
        // we capture them for partition checking.
        if (info) {
          info->hasBins = true;
          CommandNode* cn = getCommandNode(stmt);
          Expr* cond = cn->getCondition();
          if (cond && (cond->getToken() == "ID" || cond->getToken() == "VAR") &&
              getVarNode(cond)->getAccSize() >= 1) {
            // boundary list: bin VAR b0 b1 ... -> [b0,b1), ..., [bn, inf)
            VarNode* vn = getVarNode(cond);
            std::vector<int> bounds = vn->getAccessor();
            std::string key = canonicalConstraintKey(vn->getId(), drv);
            RegionBinSet set;
            set.label = vn->getId();
            for (size_t bi = 0; bi < bounds.size(); ++bi) {
              double lo = bounds[bi];
              std::vector<rf::Formula> parts;
              parts.push_back(rf::fAtom(key, rf::CmpOp::GE, lo));
              std::string lab = vn->getId() + "[" + std::to_string(bounds[bi]);
              if (bi + 1 < bounds.size()) {
                parts.push_back(
                    rf::fAtom(key, rf::CmpOp::LT, bounds[bi + 1]));
                lab += "," + std::to_string(bounds[bi + 1]) + ")";
              } else {
                lab += ",inf)";
              }
              set.bins.push_back(rf::fAnd(std::move(parts)));
              set.binLabels.push_back(lab);
            }
            if (set.bins.size() >= 1) info->binSets.push_back(std::move(set));
          } else {
            // boolean bin condition: pool all of them into one set
            RegionBinSet* pool = nullptr;
            for (auto& s : info->binSets)
              if (s.label == "conditions") pool = &s;
            if (!pool) {
              info->binSets.push_back(RegionBinSet{});
              info->binSets.back().label = "conditions";
              pool = &info->binSets.back();
            }
            pool->bins.push_back(encodeExpr(cond, ctx));
            pool->binLabels.push_back(
                "bin#" + std::to_string(pool->bins.size()));
          }
        }
      } else if (stok == "TRIGGER") {
        CommandNode* cn = getCommandNode(stmt);
        Expr* cond = cn->getCondition();
        if (info) info->selectStmts++;
        if (cond && (cond->getToken() == "ID" || cond->getToken() == "VAR")) {
          // Trigger flags are opaque per-event booleans; consistent naming
          // makes same-trigger regions comparable.
          conj.push_back(rf::fAtom("trigger(" + toupper(cond->getId()) + ")",
                                   rf::CmpOp::EQ, 1.0));
          if (info) info->selectStmtsExact++;
        } else {
          rf::Formula f = encodeExpr(cond, ctx);
          if (info && !rf::hasUnknown(f)) info->selectStmtsExact++;
          conj.push_back(std::move(f));
        }
      }
      // WEIGHT / HISTO / PRINT / SAVE / COUNTS do not constrain membership.
    }

    active.erase(rn->getId());
    rf::Formula f = rf::fAnd(std::move(conj));
    memo[rn->getId()] = f;
    return f;
  };

  for (auto& n : drv.ast) {
    if (n->getToken() != "REGION") continue;
    RegionNode* rn = getRegionNode(n);
    RegionFormulaInfo info;
    info.name = rn->getId();
    info.formula = encodeRegion(rn, &info);
    rf::countLeaves(info.formula, info.leavesTotal, info.leavesUnknown);
    rf::collectUnknownNotes(info.formula, info.dropped);
    std::sort(info.dropped.begin(), info.dropped.end());
    info.dropped.erase(std::unique(info.dropped.begin(), info.dropped.end()),
                       info.dropped.end());
    out.push_back(std::move(info));
  }
  return 0;
}

}  // namespace adl
