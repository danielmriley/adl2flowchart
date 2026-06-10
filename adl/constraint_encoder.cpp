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

bool isCompareOp(const std::string& op) {
  return op == "<" || op == "<=" || op == ">" || op == ">=" || op == "==" ||
         op == "!=" || op == "~=";
}

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

rf::Formula leafFromCompare(BinNode* bn, EncodeCtx& ctx) {
  std::string op = bn->getOp();
  Expr* lhs = bn->getLHS();
  Expr* rhs = bn->getRHS();

  double num = 0.0;
  Expr* varSide = nullptr;
  bool flipped = false;
  if (numericValueOf(rhs, num)) {
    varSide = lhs;
  } else if (numericValueOf(lhs, num)) {
    varSide = rhs;
    flipped = true;
  } else {
    return rf::fUnknown("comparison without a constant side");
  }

  std::string key = keyFromExpr(varSide, ctx);
  if (key.empty())
    return rf::fUnknown("cannot name quantity in comparison (" + op + " " +
                        fmtNum(num) + ")");
  if (!keyIsEventScalar(key, ctx, true))
    return rf::fUnknown("collection-level cut without index: " + key);

  std::string cop = op;
  if (flipped) {
    if (cop == ">") cop = "<";
    else if (cop == ">=") cop = "<=";
    else if (cop == "<") cop = ">";
    else if (cop == "<=") cop = ">=";
  }

  rf::CmpOp fop;
  if (cop == "<") fop = rf::CmpOp::LT;
  else if (cop == "<=") fop = rf::CmpOp::LE;
  else if (cop == ">") fop = rf::CmpOp::GT;
  else if (cop == ">=") fop = rf::CmpOp::GE;
  else if (cop == "==") fop = rf::CmpOp::EQ;
  else if (cop == "!=" || cop == "~=") fop = rf::CmpOp::NE;
  else return rf::fUnknown("unsupported comparison operator " + op);

  return rf::fAtom(key, fop, num);
}

// size(a) + size(b) == 0  =>  size(a) == 0 AND size(b) == 0
rf::Formula trySizeSumZero(BinNode* bn, EncodeCtx& ctx) {
  if (bn->getOp() != "==") return rf::fUnknown("");
  Expr* sumSide = nullptr;
  double num = -1.0;
  if (numericValueOf(bn->getRHS(), num)) sumSide = bn->getLHS();
  else if (numericValueOf(bn->getLHS(), num)) sumSide = bn->getRHS();
  if (!sumSide || num != 0.0) return rf::fUnknown("");
  if (sumSide->getToken() != "EXPROP") return rf::fUnknown("");
  BinNode* sum = getBinNode(sumSide);
  if (sum->getOp() != "+") return rf::fUnknown("");
  std::string ka = keyFromExpr(sum->getLHS(), ctx);
  std::string kb = keyFromExpr(sum->getRHS(), ctx);
  if (ka.rfind("size(", 0) != 0 || kb.rfind("size(", 0) != 0)
    return rf::fUnknown("");
  return rf::fAnd({rf::fAtom(ka, rf::CmpOp::EQ, 0.0),
                   rf::fAtom(kb, rf::CmpOp::EQ, 0.0)});
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
    BinNode* bn = getBinNode(e);
    rf::Formula sumZero = trySizeSumZero(bn, ctx);
    if (sumZero.kind != rf::FKind::Unknown) return sumZero;
    return leafFromCompare(bn, ctx);
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
        // Bins partition a region; they do not constrain membership.
        if (info) info->hasBins = true;
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
