#include "driver.h"

namespace adl {

  std::string toupper(std::string s) {
    for(int i = 0; i < s.size(); i++) {
      s[i] = std::toupper(s[i]);
    }
    return s;
  }

  std::string tolower(std::string s) {
    for(int i = 0; i < s.size(); i++) {
      s[i] = std::tolower(s[i]);
    }
    return s;
  }

  Driver::Driver(std::istream *in) : scanner(*this, in), parser(scanner, *this), loc(0) {
    fillTypeTable();
    
    // Set file path to the current working directory.
    std::filesystem::path p = std::filesystem::current_path();

    while (true)
    {
      std::cout << "Checking path: " << p << "\n";
      std::filesystem::path potential_lib_path = p / "adl";
      // Check if "adl" exists and is a directory.
      if (std::filesystem::exists(potential_lib_path) && std::filesystem::is_directory(potential_lib_path))
      {
        libPath = potential_lib_path;
        break;
      }
      // Move to the parent directory.
      auto parent = p.parent_path();
      // If we've reached the filesystem root (parent is the same as current), stop.
      if (parent == p)
      {
        throw std::runtime_error("adl directory not found in any ancestor directory");
      }
      p = parent;
    }

    std::cout << "LIBPATH: " << libPath << "\n";
  }

  int Driver::parse() {
    loc = 0;
    return parser.parse();
  }

  int Driver::parse(std::string fileName) {
    loc = 0;

    return parser.parse();
  }

  int Driver::check_function_table(std::string id) {
    std::ifstream fin(libPath.string() + "/" + functionsLib);
    std::string input;

    while(fin >> input) {
      if(id == input) {
        std::cout << "function " << id << " is REGISTERED\n";
        fin.close();
        return 0;
      }
    }
    std::cout << "ERROR: external function " << id << " is not found\n";
    fin.close();
    return 1;
  }

  int Driver::check_property_table(std::string id) {
    std::ifstream fin(libPath.string() + "/" + propertiesLib);
    std::string input;
    id = toupper(id);

    while(fin >> input) {
      input = toupper(input);
      if(id == input) {
        std::cout << id << " is a PROPERTY\n";
        fin.close();
        return 0;
      }
    }
    std::cout << id << " is not a property\n";
    fin.close();
    return 1;
  }

  int Driver::check_object_table(std::string id) {
    id = toupper(id);
    auto itr = objectTable.find(id);
    if(itr == objectTable.end()) {
      return 1;
    }
    return 0;
  }

  void Driver::fillTypeTable() {
    typeTable["NONE"] = none_t;
    typeTable["ELE"] = electron_t;
    typeTable["ELECTRON"] = electron_t;
    typeTable["JET"] = jet_t;
    typeTable["QCJET"] = lightjet_t;
    typeTable["MUOLIKE"] = muonlikeV_t;
    typeTable["MUONLIKE"] = muonlikeV_t;
    typeTable["ELELIKE"] = electronlikeV_t;
    typeTable["ELECTRONLIKE"] = electronlikeV_t;
    typeTable["PUREV"] = pureV_t;
    typeTable["METLV"] = pureV_t;
    typeTable["PHO"] = photon_t;
    typeTable["PHOTON"] = photon_t;
    typeTable["FJET"] = fjet_t;
    typeTable["TRUTH"] = truth_t;
    typeTable["TAU"] = tau_t;
    typeTable["MUO"] = muon_t;
    typeTable["MUON"] = muon_t;
    typeTable["TRACK"] = track_t;
    typeTable["TRK"] = track_t;
    typeTable["COMB"] = combo_t;
    typeTable["COMBO"] = combo_t;
    typeTable["CONSTIT"] = consti_t;
    typeTable["MISSINGET"] = pureV_t;
    typeTable["MET"] = pureV_t;
    typeTable["MHT"] = pureV_t;
    typeTable["DELPHES_MISSINGET"] = pureV_t;
    typeTable["DELPHES_SCALARHT"] = pureV_t;
    typeTable["SCALARHT"] = pureV_t;
    typeTable["AK4JET"] = jet_t;
    typeTable["AK8JET"] = jet_t;
    typeTable["FATJET"] = fjet_t;
    typeTable["FJET"] = fjet_t;
    typeTable["BJET"] = bjet_t;
    typeTable["DELPHES_JET"] = jet_t;
    typeTable["DELPHES_ELECTRON"] = electron_t;
    typeTable["DELPHES_MUON"] = muon_t;
    typeTable["DELPHES_PHOTON"] = photon_t;
    typeTable["JETS"] = jet_t;
    typeTable["MUONS"] = muon_t;
    typeTable["ELECTRONS"] = electron_t;
    typeTable["LEPTON"] = electron_t;
    typeTable["LEPTONS"] = electron_t;
    typeTable["PHOTONS"] = photon_t;
    typeTable["HT"] = pureV_t;
    typeTable["ST"] = pureV_t;
    typeTable["FHT"] = pureV_t;
  }

  int Driver::visitAST(int (*f)(ExprVector& _ast)) {
    return f(ast);
  }

  void Driver::loadFromLibraries() {
    std::string path = libPath.string() + "/" + objsLib;
    std::cout << "PATH: " << path << "\n";
    std::ifstream fin(path);
    if(!fin.good()) {
      std::cerr << "ERROR: Cannot load library\n";
    }
    std::string input;

    while(fin >> input) {
      input = toupper(input);
      addObject(input,std::string("PARENT"));
      dependencyChart[input].push_back(input);
    }
    fin.close();
  }

  std::string Driver::getBinType(Expr* expr) {
    std::string lhsType, rhsType;
    if(binOpCheck(expr) == 0) {
      BinNode* binExpr = static_cast<BinNode*>(expr);
      lhsType = getBinType(binExpr->getLHS());
      rhsType = getBinType(binExpr->getRHS());
      if(lhsType == rhsType) return lhsType;
      else {
        std::cout << "ERROR: There is a type mismatch\n";
        return "";
      }
    }
    else { return static_cast<VarNode*>(expr)->getType(); }
  }

  std::string Driver::getObjectDeclType(std::string s) {
    for(auto &o: objectTable) {
      if(toupper(o.first) == toupper(s)) {
        return o.second;
      }
    }
    return "NOT FOUND";
  }

  int Driver::setTables() {
    loadFromLibraries();
    for(int i = 0; i < ast.size(); i++) {
      std::string token = ast[i]->getToken();
      if(token == "DEFINE") {
        std::string defType;
        if(binOpCheck(ast[i]) == 0) {
          defType = getBinType(ast[i]);
        }
        else if(ast[i]->getToken() == "ID") {
          defType = static_cast<VarNode*>(ast[i])->getType();
        }
        std::cout << "ADDING DEFINE: " << ast[i]->getId() << "\n";
        addDefine(ast[i]->getId());
      }
      else if(token == "OBJECT") {
        std::string takeType;
        ExprVector stmnts = static_cast<astObjectNode*>(ast[i])->getStatements();
        for(auto& e: stmnts) {
          if(e->getToken() == "TAKE") {
            CommandNode* cn = static_cast<CommandNode*>(e);
            VarNode* vn = static_cast<VarNode*>(cn->getCondition());
            takeType = vn->getId();
          }
        }
        addObject(ast[i]->getId(),takeType);
      }
      else if(token == "REGION") {
        addRegion(ast[i]->getId());
        // add region's vars to "region vars" table.
      }
      else if(token == "HISTOLIST") {
        addRegion(ast[i]->getId());
        // add region's vars to "region vars" table.
      }
    }
    return 0;
  }

  int Driver::checkObjectTable(std::string id) {
    for(auto& e: objectTable) {
      std::string eUpper = adl::toupper(e.first);
      std::string idUpper = adl::toupper(id);
      if(eUpper == idUpper) {
        // std::cout << "Object " << id << " has been declared\n";
        return 0;
      }
    }
    // std::cout << "ERROR: Object " << id << " NOT declared\n";
    return 1;
  }

  int Driver::checkDefinitionTable(std::string id) {
    for(auto e: definitionTable) {
      std::string eUpper = adl::toupper(e);
      std::string idUpper = adl::toupper(id);
      if(eUpper == idUpper) {
        // std::cout << "Variable " << id << " has been declared\n";
        return 0;
      }
    }
    // std::cout << "ERROR: Variable " << id << " NOT declared\n";
    return 1;
  }

  int Driver::checkRegionTable(std::string id) {
    for(auto e: regionTable) {
      std::string eUpper = adl::toupper(e);
      std::string idUpper = adl::toupper(id);
      if(eUpper == idUpper) {
        // std::cout << "Region " << id << " has been declared\n";
        return 0;
      }
    }
    // std::cout << "ERROR: Region " << id << " NOT declared\n";
    return 1;
  }

  void Driver::addNode(Expr* node) {
    ast.push_back(node);
  }

  int Driver::addDefine(std::string id) {
    // Check that the definition isn't already in the table.
    for(auto e: definitionTable){
      if(e == id) {
        // std::cout << "ERROR: Variable " << id << "  has been previously defined\n";
        return 1;
      }
    }
    definitionTable.push_back(id);
    return 0;
  }

  int Driver::addObject(std::string id,std::string takeType) {
    // Check that the definition isn't already in the table.
    for(auto e: objectTable){
      if(e.first == id) {
        // std::cout << "ERROR: Object " << id << " has been previously defined\n";
        return 1;
      }
    }
    objectTable[id] = takeType;
    return 0;
  }

  int Driver::addRegion(std::string id) {
    // Check that the definition isn't already in the table.
    for(auto e: regionTable){
      if(e == id) {
        // std::cout << "ERROR: Region " << id << "  has been previously defined\n";
        return 1;
      }
    }
    regionTable.push_back(id);
    return 0;
  }

  void Driver::incrementLocation(unsigned int l) {
    loc+=l;
  }

  unsigned int Driver::location() {
      return loc;
  }

  std::string Driver::findDep(std::string var) {
    std::string type = "";
    for(auto &d : dependencyChart) {
      for(auto &v : d.second) {
        if(toupper(v) == toupper(var)) {
          type = d.first;
          return type;
        }
      }
    }
    return type;
  }

  void Driver::processDefBinNode(DefineNode* dn, Expr* body) {
    BinNode* bn = static_cast<BinNode*>(body);
    Expr* rhs = bn->getRHS();
    Expr* lhs = bn->getLHS();

    if(binOpCheck(rhs) == 0) { processDefBinNode(dn, rhs); }
    if(binOpCheck(lhs) == 0) { processDefBinNode(dn, lhs); }

    std::string type;
    if(rhs->getToken() == "ID") {
      type = findDep(rhs->getId());
      if(type != "") {
        dn->setType(type);
      }
      else {
        dn->setType("OBJECT");
      }
    }
    if(lhs->getToken() == "ID") {
      type = findDep(lhs->getId());
      if(type != "") {
        dn->setType(type);
      }
      else {
        dn->setType("OBJECT");
      }
    }
    if(rhs->getToken() == "FUNCTION") {
      dn->setType("REAL");
    }
    if(lhs->getToken() == "FUNCTION") {
      dn->setType("REAL");
    }
    if(rhs->getToken() == "REAL") {
      dn->setType("REAL");
    }
    if(lhs->getToken() == "REAL") {
      dn->setType("REAL");
    }
    if(rhs->getToken() == "INT") {
      dn->setType("REAL");
    }
    if(lhs->getToken() == "INT") {
      dn->setType("REAL");
    }
  }

  void Driver::setDependencyChart() {
    // Fill out the dep chart by a traversal of the ast.
    // Set the types of Objects and Definitions.
    for(auto &n : ast) {
      if(n->getToken() == "DEFINE") {
        DefineNode* dn = static_cast<DefineNode*>(n);
        Expr* body = dn->getBody();
        if(body->getToken() == "FUNCTION"
           || body->getToken() == "REAL"
           || body->getToken() == "INT") {
          dn->setType("REAL");
        }
        if(binOpCheck(body) == 0) {
          processDefBinNode(dn, body);
        }
      }
      if(n->getToken() == "OBJECT") {
        astObjectNode* on = getObjectNode(n);
        auto stmnts = on->getStatements();

        for(auto &s: stmnts) {
          if(s->getToken() == "TAKE") {
            Expr* cond = static_cast<CommandNode*>(s)->getCondition();
            std::string var;
            if(cond && cond->getToken() == "FUNCTION") {
              var = getFunctionNode(cond)->getId();
            } else if(cond) {
              var = cond->getId();
            }
            std::string varDeclType = getObjectDeclType(var);
            std::cout << "VARDECL TYPE: " << varDeclType << "\n";
            if(varDeclType != "NOT FOUND" && varDeclType == "PARENT") {
              on->setObjectType(var);
              dependencyChart[toupper(var)].push_back(on->getId());
            }
            else if(checkObjectTable(var) == 0) { // Here means its a declared type.
              for(auto &p: dependencyChart) {
                auto itr = std::find(p.second.begin(), p.second.end(), var);
                if(itr != p.second.end()) {
                  on->setObjectType(p.first);
                  dependencyChart[toupper(p.first)].push_back(on->getId());
                  break;
                }
              }
            }
            else {
              std::cout << "Not an object\n";
            }
          }
        }
      }
    } // end loop.
    std::cout << "\n==== dependency chart ====\n\n";
    for(auto &d : dependencyChart) {
      std::cout << d.first << "\n  ";

      for(auto &v : d.second) {
        std::cout << v << ", ";
      }
      std::cout << "\n";
    }
    std::cout << "\n";
  }

  std::string Driver::getVarNodeType(std::string vn) {
    for(auto& chart: dependencyChart) {
      for(auto& v: chart.second) {
        if(toupper(v) == toupper(vn)) {
          return chart.first;
        }
      }
    }
    return "";
  }

  // CutLang lowering (ast2cuts, makeNode, particle factories) removed:
  // it was unreachable from main and stubbed at every leaf.
} // end namespace adl
