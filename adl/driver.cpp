#include "driver.h"

int cutcount;
int bincount;

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
    fillParentObjectsMap();

    parts = new std::list<std::string>;
    NodeVars = new std::map<std::string,Node*>;
    ListParts = new std::map<std::string, std::vector<myParticle*> >;
    NodeCuts = new std::map<int,Node*>;
    BinCuts = new std::map<int,Node*>;
    ObjectCuts = new std::map<std::string,Node*>;
    Initializations = new std::vector<std::string>;
    TRGValues = new std::vector<int>;
    ListTables = new std::map<std::string, std::pair<std::vector<float>, bool> >;
    cntHistos = new std::map<std::string, std::vector<cntHisto> >;
    systmap = new std::map<int, std::vector<std::string> >;

    // Set file path.
    std::filesystem::path p = std::filesystem::current_path();
    while(true) {
      std::cout << "PATH: " << p << "\n";
      auto pitr = p.end();
      pitr--;
      if(pitr->string() == "adl2flowchart") {
        break;
      }
      p = p.parent_path();
    }
    std::filesystem::path lib_path(p.string() + "/adl");
    std::cout << "LIBPATH: " << lib_path << "\n";
    libPath = lib_path;

  }

  int Driver::parse() {
    loc = 0;
    return parser.parse();
  }

  int Driver::parse(std::string fileName) {
    loc = 0;
    // if(fileName == "") {
    //   scanner.yyin = stdin;
    // }
    // else {
    //   scanner.yyin = fopen(fileName.c_str(), "r");
    // }

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

  void Driver::fillParentObjectsMap() {
    std::vector<Node*> newList;

    parentParticleObjects["ELE"] = new ObjectNode("ELE", NULL, createNewEle, newList, "obj ELE" );
    parentParticleObjects["TRUTH"]  = new ObjectNode("Truth", NULL, createNewMuo, newList, "obj Truth" );
    parentParticleObjects["TRACK"]  = new ObjectNode("Track", NULL, createNewMuo, newList, "obj Track" );
    parentParticleObjects["MUO"]  = new ObjectNode("MUO", NULL, createNewEle, newList, "obj MUO" );
    parentParticleObjects["TAU"]  = new ObjectNode("TAU", NULL, createNewEle, newList, "obj TAU" );
    parentParticleObjects["PHO"]  = new ObjectNode("PHO", NULL, createNewEle, newList, "obj PHO" );
    parentParticleObjects["JET"]  = new ObjectNode("JET", NULL, createNewEle, newList, "obj JET" );
    parentParticleObjects["FJET"] = new ObjectNode("FJET", NULL, createNewEle, newList, "obj FATJET" );
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
            std::string var = cond->getId();
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

  myParticle* Driver::createParticle(VarNode* vn) {
    myParticle* part = new myParticle;
    std::string type = getVarNodeType(vn->getId());
    std::cout << "VARTYPE: " << type << "\n";
    part->type = typeTable[toupper(type)];
    std::cout << "TYPENUM: " << part->type << std::endl;
    std::vector<int> ind = vn->getAccessor();
    std::cout << "createParticle" << std::endl;
    if(ind.size() > 0) {
      part->index = ind[0]; // DR: temporary. Needs to accomodate range of indecies.
    }
    else {
      part->index = 6213;
    }
    std::cout << "INDEX: " << part->index << std::endl;
    part->collection = vn->getId();
    std::cout << "COLLECTION: " << part->collection << std::endl;

    return part;
  }

  Node* Driver::getFuncNode(Expr* f) {
    Node* node = nullptr;
    FunctionNode* fn = getFunctionNode(f); // static cast helper.
    std::vector<myParticle*> particlesList;
    std::string funcName = fn->getId();
    funcName = tolower(funcName);
    std::cout << "LOOKING FOR FUNCTION: " << funcName << "\n";

    auto funcItr = function_map.find(funcName);
    auto lFuncItr = lfunction_map.find(funcName);
    auto uFuncItr = unfunction_map.find(funcName);
    auto sFuncItr = sfunction_map.find(funcName);

    ExprVector params = fn->getParams();

    if(funcName == "size") {
      std::cout << "IN SIZE FUNC BRANCH\n";
      VarNode* param = getVarNode(params[0]);
      std::cout << "SIZE(PARAM): " << param->getId() << "\n";
      auto ito = ObjectCuts->find(param->getId());
      if(ito != ObjectCuts->end()) {
        int type=((ObjectNode*)ito->second)->type;
        node = new SFuncNode(count, type, ito->first, ito->second);
      }
      else {
        std::vector<myParticle*> newList;
        newList.push_back(createParticle(param));
        node = new SFuncNode(count, newList[0]->type, newList[0]->collection);
      }
    }
    else if(funcItr != function_map.end()) { // Particle attribute
      for(auto& p : params) {
        VarNode* param = getVarNode(p);
        // Fill particlesList with the particles that are params.
        std::cout << "PARAM: " << param->getId() << "\n";
        std::cout << "TYPE: " << findDep(param->getId()) << "\n";
        myParticle* part = createParticle(param);
        particlesList.push_back(part);
      }
      node = new FuncNode(function_map[funcName],particlesList,funcName);
    }
    else if(lFuncItr != lfunction_map.end()) {
      std::cout << "NEED TO MAKE AN LFUNCNODE\n";

      VarNode* param = getVarNode(params[1]);
      // Fill particlesList with the particles that are params.
      std::cout << "PARAM: " << param->getId() << std::endl;;
      std::cout << "TYPE: " << findDep(param->getId()) << "\n";
      myParticle* part = createParticle(param);
      particlesList.push_back(part);

      auto partlist2 = particlesList;
      particlesList.clear();

      VarNode* param1 = getVarNode(params[1]);
      // Fill particlesList with the particles that are params.
      std::cout << "PARAM: " << param1->getId()  << std::endl;;
      std::cout << "TYPE: " << findDep(param1->getId()) << "\n";
      part = createParticle(param1);
      particlesList.push_back(part);

      node = new LFuncNode(lfunction_map[funcName], partlist2, particlesList, funcName);
    }
    else if(uFuncItr != unfunction_map.end()) { // Unary functions
      node = new UnaryAONode(unfunction_map[funcName], makeNode(params[0]), funcName);
    }
    else if(sFuncItr != sfunction_map.end()) { // "special"(?) functions
      if(funcName == "met" || funcName == "all" || funcName == "none") {
        node = new SFuncNode(sfunction_map[funcName], 1, funcName);
      }
      if(funcName == "metsig") {
        node = new SFuncNode(sfunction_map[funcName], 3.1416, funcName);
      }
    }
    // Start serach for match to Razor funcs
    else if(funcName == "fmr") {
      std::string name = params[0]->getId();
      std::cout << "INSERT FMR PARAM: " << name << "\n";
      std::map<std::string,Node*>::iterator it = ObjectCuts->find(name);
      int type = -1;

      if(it == ObjectCuts->end()) {
        std::cerr << "UNCAUGHT ERROR fmr" << name << "\n";
      }
      else {
        std::cout << "setting type\n";
        type = ((ObjectNode*)it->second)->type;
      }
      std::cout << "setting fmr SFuncNode\n";
      node = new SFuncNode(userfuncB, fMR, type, name, it->second);
    }
    else if(funcName == "fmegajets") {
      std::cout << "INSERT FMEGAJETS\n";
      std::string name = params[0]->getId();
      std::map<std::string,Node*>::iterator it = ObjectCuts->find(name);
      int type = -1;

      if(it == ObjectCuts->end()) {
        std::cerr << "UNCAUGHT ERROR fmegajets" << name << "\n";
      }
      else {
        type = ((ObjectNode*)it->second)->type;
      }
      node = new SFuncNode(userfuncA, fmegajets, type, "MEGAJETS", it->second);
    }
    else if(funcName == "fmtr") {
      std::cout << "INSERT FMTR" << std::endl;
      std::cout << "PARAMS SIZE: " << params.size() << std::endl;
      std::string name = params[1]->getId();
      std::cout << "PARAM NAME1: " << params[1]->getId() << std::endl;
      std::map<std::string,Node*>::iterator it = ObjectCuts->find(name);
      std::map<std::string,std::vector<myParticle*> >::iterator it2;
      int type = -1;

      if(it == ObjectCuts->end()) {
        std::cerr << "UNCAUGHT ERROR fmtr" << name << "\n";
      }
      else {
        type = ((ObjectNode*)it->second)->type;
      }

      if(tolower(params[0]->getId()) == "met") {
        std::cout << "PARAM NAME2: " << params[0]->getId() << std::endl;
        node = new SFuncNode(userfuncC, fMTR, type, name, it->second);
      }
      else {
        it2 = ListParts->find(params[0]->getId());
        node = new SFuncNode(userfuncD, fMTR2, type, name, it2->second, it->second);
      }

    }
    else if(funcName == "fmt") {
      std::cout << "INSERT FMT\n";
    }
    if(node == nullptr) std::cout << "RETURNING A NULL FUNCTION NODE\n";
    return node;
  }

  Node* Driver::makeNode(Expr* expr) {
    // expr should be the RHS of an equal sign.
    std::cout << "EXPR ID: " << expr->getId() << "\n";
    Node* node = nullptr;
    if(binOpCheck(expr) == 0) {
      BinNode* bn = static_cast<BinNode*>(expr);
      Expr* lhs = bn->getLHS();
      Expr* rhs = bn->getRHS();

      if(bn->getOp() == "+") {
        node = new BinaryNode(add, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == "-") {
        node = new BinaryNode(sub, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == "*") {
        node = new BinaryNode(mult, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == "/") {
        node = new BinaryNode(div, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == "^") {
        node = new BinaryNode(pow, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == ">") {
        node = new BinaryNode(gt, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == "<") {
        node = new BinaryNode(lt, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == ">=") {
        node = new BinaryNode(ge, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == "<=") {
        node = new BinaryNode(le, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == "==") {
        node = new BinaryNode(eq, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == "!=") {
        node = new BinaryNode(ne, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == "&&" || toupper(bn->getOp()) == "AND") {
        node = new BinaryNode(LogicalAnd, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == "||" || toupper(bn->getOp()) == "OR") {
        node = new BinaryNode(LogicalOr, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
    }

    else if(expr->getToken() == "FUNCTION") {
      std::cout << "Making a FUNCTION NODE";
      FunctionNode* fn = getFunctionNode(expr);
      std::cout << "  : " << fn->getId() << std::endl;
      node = getFuncNode(fn);
      if(node == nullptr) std::cout << "AFTER FUNCTION MAKING NULLPTR\n";
    }
    else if(toupper(expr->getId()) == "ALL"
            || toupper(expr->getId()) == "NONE"
            || toupper(expr->getId()) == "MET"
            || toupper(expr->getId()) == "FHT") {
      std::cout << "Found ALL or NONE\n";
      std::cout << "  : " << expr->getId() << std::endl;
      std::string param;
      if(toupper(expr->getId()) == "FHT") param = "JET";
      else param = expr->getId();
      node = new SFuncNode(sfunction_map[tolower(expr->getId())], 1, param);
    }
    else if(expr->getToken() == "ID") {
      std::cout << "FOUND AN ID TOKEN\n";
      std::map<std::string, Node*>::iterator it;
      it = NodeVars->find(expr->getId());
      std::map<std::string, Node*>::iterator ito;
      ito = ObjectCuts->find(expr->getId());
      if(it != NodeVars->end()) {
        std::cout << "Found a NODE" << std::endl;
        std::cout << "  : " << expr->getId() << std::endl;
        node = it->second;
      }
      if(ito != ObjectCuts->end()) {
        std::cout << "Found an OBJ NODE\n";
        std::cout << "  : " << expr->getId() << std::endl;
        node = ito->second;
      }
    }
    else if(expr->getToken() == "REAL" || expr->getToken() == "INT") {
      std::cout << "Making a VALUE NODE\n";
      std::cout << "  : " << expr->getId() << std::endl;
      node = new ValueNode(expr->value());
    }

    else if(expr->getToken() == "ITE") {
      std::cout << "Making a ITE NODE\n";
      std::cout << "  : " << expr->getId() << std::endl;
      ITENode* ite = getITENode(expr);
      node = new IfNode(makeNode(ite->getCondition()), makeNode(ite->getThenBranch()), makeNode(ite->getElseBranch()), "if");
    }
    if(node == nullptr) {
      std::cout << "**** NODE IS NULLPTR **** => ";
      std::cout << "  : " << expr->getId() << " ==> " << expr->getToken() << std::endl;
    }
    std::cout << "\n";
    return node;
  }

  Node* Driver::createParentObject(std::string id) {
    id = toupper(id);
    std::cout << "NAME: " << id << std::endl;;
    Node* node = nullptr;
    std::vector<Node*> newList;
    if(toupper(id) == "ELE" || toupper(id) == "ELECTRON") {
      node = new ObjectNode("ELE", NULL, createNewEle, newList, "obj ELE");
    }
    else if(toupper(id) == "MUO" || toupper(id) == "MUON") {
      node = new ObjectNode("MUO", NULL, createNewMuo, newList, "obj MUO");
    }
    else if(toupper(id) == "PHO" || toupper(id) == "PHOTON") {
      node = new ObjectNode("PHO", NULL, createNewPho, newList, "obj PHO");
    }
    else if(toupper(id) == "TRK" || toupper(id) == "TRACK") {
      node = new ObjectNode("Track", NULL, createNewTrack, newList, "obj TRACK");
    }
    else if(toupper(id) == "FJET" || toupper(id) == "FATJET") {
      node = new ObjectNode("FJET", NULL, createNewFJet, newList, "obj FatJet");
    }
    else if(toupper(id) == "TAU") {
      node = new ObjectNode("TAU", NULL, createNewTau, newList, "obj TAU");
    }
    else if(toupper(id) == "TRUTH") {
      node = new ObjectNode("Truth", NULL, createNewTruth, newList, "obj Truth");
    }
    else if(toupper(id) == "JET") {
      node = new ObjectNode("JET", NULL, createNewJet, newList, "obj JET");
    }
    return node;
  }

  void Driver::processObject(astObjectNode* on) {
    ExprVector stmnts = on->getStatements();

    Node* parent = nullptr;
    Node* obj = nullptr;
    std::string parentStr;
    std::vector<Node*> newList;
    for(auto& s: stmnts) {
      if(s->getToken() == "TAKE") {
        CommandNode* cn = getCommandNode(s);
        Expr* cond = cn->getCondition();
        if(cond->getToken() == "ID") {
          VarNode* vn = getVarNode(cond);
          std::string type = getObjectDeclType(vn->getId());
          if(toupper(type) == "PARENT") {
            parentStr = cond->getId();
            std::cout << "CREATED PARENT NODE\n";
            parent = createParentObject(cond->getId());
            // Fill up newList.
            // obj = new ObjectNode(on->getId(), parent, NULL, newList, on->getId());
            // ObjectCuts->insert(std::make_pair(on->getId(), obj));
          }
          else {
            std::map<std::string, Node *>::iterator it;
            it = ObjectCuts->find(vn->getId());
            if(it != ObjectCuts->end()) {
              // Need to fill newList with the nodes from the object statements.
              std::cout << "FOUND NON PARENT INHERITANCE\n";
              parent = it->second;
              // obj = new ObjectNode(on->getId(), parent, NULL, newList, on->getId());
              // ObjectCuts->insert(std::make_pair(on->getId(), obj));
            }
            else {
              std::cerr << "***** An ERROR that has not previously been caught.\n";
            }
          }
        }
        else {
          std::cerr << "There could be an error in the TAKE statement.\n";
        }
      }
      else {
        CommandNode* cn = getCommandNode(s);
        Expr* condition = cn->getCondition();
        std::cout << "condition is: " << condition->getToken() << " | " << condition->getId() << " |\n";
        newList.push_back(makeNode(cn->getCondition()));
      }
    }
    obj = new ObjectNode(on->getId(), parent, NULL, newList, on->getId());
    ObjectCuts->insert(std::make_pair(on->getId(), obj));
  }

  void Driver::gatherParticles(Expr* body, std::vector<myParticle*> &particles) {
    if(binOpCheck(body) == 0) {
      BinNode* bn = getBinNode(body);
      Expr* lhs = bn->getLHS();
      Expr* rhs = bn->getRHS();
      if(binOpCheck(lhs) == 0) {
        gatherParticles(lhs, particles);
      }
      else {
        VarNode* vn = getVarNode(lhs);
        std::cout << "VAR: " << vn->getId() << std::endl;
        myParticle* part = createParticle(vn);
        particles.push_back(part);
      }
      if(binOpCheck(rhs) == 0) {
        gatherParticles(rhs, particles);
      }
      else {
        VarNode* vn = getVarNode(rhs);
        std::cout << "VAR: " << vn->getId() << std::endl;
        myParticle* part = createParticle(vn);
        particles.push_back(part);
      }
    }
    // else {
    //   std::cout << "NEEDS TO BE IMPLEMENTED!" << std::endl;
    // }
  }

  void Driver::processRegion(RegionNode* rn) {
    ExprVector stmnts = rn->getStatements();
    int cutcount = 0;
    for(auto& s: stmnts) {
      CommandNode* cn = getCommandNode(s);
      std::cout << "COMMAND: " << cn->getToken() << "\n";
      if(cn->getToken() == "HISTO") {
        HistoNode* hn = getHistoNode(cn);
        ExprVector params = hn->getParams();
        std::cout << "PARAMS.SIZE(): " << params.size() << "\n";
        if(params.size() == 4) {
          std::cout << "MAKING HISTO1D\n";
          Node* node = new HistoNode1D(hn->getId(), hn->getDescription(), params[3]->value(), params[2]->value(), params[1]->value(), makeNode(params[0]));
          NodeCuts->insert(std::make_pair(++cutcount, node));
        }
        if(params.size() == 8) {
          std::cout << "MAKING HISTO2D\n";
          Node* node = new HistoNode2D(hn->getId(), hn->getDescription(), params[7]->value(), params[6]->value(), params[5]->value(), params[4]->value(), params[3]->value(), params[2]->value(), makeNode(params[1]), makeNode(params[0]));
          NodeCuts->insert(std::make_pair(++cutcount, node));
        }
      }
      if(cn->getToken() == "SELECT") {
        std::cout << "HERE" << std::endl;
        Expr* cond = cn->getCondition();
        if(checkRegionTable(cond->getId()) == 0) {
          continue;
        }

        Node* node = nullptr;
        node = makeNode(cond);
        if(node == nullptr) std::cout << "inserted a NULLPTR\n";
        NodeCuts->insert(std::make_pair(++cutcount, node));
      }
    }
  }

  int Driver::ast2cuts(std::list<std::string> *_parts,std::map<std::string,Node*>* _NodeVars,
                       std::map<std::string, std::vector<myParticle*> >* _ListParts,
                       std::map<int,Node*>* _NodeCuts,
                       std::map<int,Node*>* _BinCuts,
                       std::map<std::string,Node*>* _ObjectCuts,
                       std::vector<std::string>* _Initializations,
                       std::vector<int>* _TRGValues,
                       std::map<std::string, std::pair<std::vector<float>, bool> >* _ListTables,
                       std::map<std::string, std::vector<cntHisto> >* _cntHistos,
                       std::map<int, std::vector<std::string> > *_systmap)
  {
    std::cout << "\n==== ast2cuts ====\n\n";

    fillFuncMaps(function_map, lfunction_map, unfunction_map, sfunction_map);
    // for(auto &fm: function_map) std::cout << fm.first << "\n";


    for(auto& a: ast) { // Loop through the AST and fill in data structures.
      if(a->getToken() == "DEFINE") {
        DefineNode* dn = static_cast<DefineNode*>(a);
        VarNode* varNode = getVarNode(getDefineNode(a)->getVar());
        std::string name = varNode->getId();
        std::cout << "DEF NAME: " << name << "\n";
        std::cout << "DEF TYPE: " << varNode->getType() << "\n";
        // pnum = 0;
        parts->push_back(name + " : " + "");

        if(varNode->getType() == "REAL") {
          // Make a node out of the RHS of the = or :
          Node* node = nullptr;
          std::cout << "DN BODY: " << dn->getBody()->getId() << "\n";
          node = makeNode(dn->getBody());
          if(node == nullptr) std::cout << "inserted a NULLPTR\n";
          NodeVars->insert(std::make_pair(name, node));
          auto it = NodeVars->find(name);
          if(it != NodeVars->end())
            std::cout << "INSERTED: " << name << "\n\n";
        }
        else { // Means it's a particle definition.
          std::cout << "*** ADDING TO LISTPARTS ***\n";
          std::vector<myParticle*> particles;
          gatherParticles(dn->getBody(), particles);
          for(auto& p : particles) if(p == nullptr) std::cout << "inserted a NULLPTR\n";
          ListParts->insert(std::make_pair(name, particles));
        }
      }
      if(a->getToken() == "REGION") {
        RegionNode *rn = getRegionNode(a);
        std::string name = rn->getId();
        std::cout << "REG NAME: " << name << "\n";
        processRegion(rn);

      }
      if(a->getToken() == "OBJECT") {
        astObjectNode *on = getObjectNode(a);
        std::string name = on->getId();
        std::cout << "OBJ NAME: " << name << "\n";
        std::cout << "OBJ TYPE: " << on->getType() << "\n";
        processObject(on);
        std::cout << "END OBJECT PROCESSING\n";

      }
    }
    *_parts = *parts;
    *_NodeVars = *NodeVars;
    *_ListParts = *ListParts;
    *_NodeCuts = *NodeCuts;
    *_BinCuts = *BinCuts;
    *_ObjectCuts = *ObjectCuts;
    *_Initializations = *Initializations;
    *_TRGValues = *TRGValues;
    *_ListTables = *ListTables;
    *_cntHistos = *cntHistos;
    *_systmap = *systmap;



    // std::cout << "\n\nPART: ";
    // for(auto& l: *_ListParts) std::cout << l.first << ", ";
    // std::cout << "\n\nOBJ: ";
    // for(auto& l: *_ObjectCuts) std::cout << l.first << ", ";
    // std::cout << "\n\nNODE: ";
    // for(auto& l: *_NodeVars) std::cout << l.first << ", ";
    // std::cout << "\n";


    return 0;
  }

  void Driver::fillFuncMaps(std::map<std::string, PropFunction> &function_map,
                    std::map<std::string, LFunction> &lfunction_map,
                    std::map<std::string, UnFunction> &unfunction_map,
                    std::map<std::string, SFunction> &sfunction_map) {
    // load the function pointers into their respective maps.
    function_map["mass"] = Mof;
    function_map["m"] = Mof;
    function_map["mass"] = Mof;
    function_map["q"] = Qof;
    function_map["charge"] = Qof;
    function_map["constituents"] = CCountof;
    function_map["daughters"] = CCountof;
    function_map["pdgid"] = pdgIDof;
    function_map["index"] = IDXof;
    function_map["p"] = Pof;
    function_map["e"] = Eof;
    function_map["tautag"] = isTauTag;
    function_map["btag"] = isBTag;
    function_map["ctag"] = isBTag;
    function_map["btagdeepb"] = DeepBof;
    function_map["msoftdrop"] = MsoftDof;
    function_map["tau1"] = tau1of;
    function_map["tau2"] = tau2of;
    function_map["tau3"] = tau3of;
    function_map["dxy"] = dxyof;
    function_map["edxy"] = edxyof;
    function_map["edz"] = edzof;
    function_map["dz"] = dzof;
    function_map["vertexr"] = vtrof;
    function_map["vertexz"] = vzof;
    function_map["vertexy"] = vyof;
    function_map["vertexx"] = vxof;
    function_map["vertext"] = vtrof;
    function_map["subjet1btag"] = sub1btagof;
    function_map["subjet2btag"] = sub2btagof;
    function_map["mvaloose"] = mvalooseof;
    function_map["mvatight"] = mvatightof;
    function_map["sieie"] = sieieof;
    function_map["minipfrelisoall"] = relisoof;
    function_map["relisoall"] = relisoallof;
    function_map["pfreliso03all"] = pfreliso03allof;
    function_map["iddecaymode"] = iddecaymodeof;
    function_map["idantieletight"] = idantieletightof;
    function_map["idantimutight"] = idantimutightof;
    function_map["tightid"] = tightidof;
    function_map["puid"] = puidof;
    function_map["genpartidx"] = genpartidxof;
    function_map["decaymode"] = decaymodeof;
    function_map["truthparentid"] = truthParentIDof;
    function_map["truthid"] = truthIDof;
    function_map["truthmatchprob"] = truthMatchProbof;
    function_map["averagemu"] = averageMuof;
    function_map["softid"] = softIdof;
    function_map["status"] = softIdof;
    function_map["dmvanewdm2017v2"] = tauisoof;
    function_map["phi"] = Phiof;
    function_map["rap"] = Rapof;
    function_map["eta"] = Etaof;
    function_map["abseta"] = AbsEtaof;
    function_map["ptcone"] = PtConeof;
    function_map["etcone"] = EtConeof;
    function_map["isolationvar"] = IsoVarof;
    function_map["miniiso"] = MiniIsoVarof;
    function_map["tight"] = isTight;
    function_map["medium"] = isMedium;
    function_map["loose"] = isLoose;
    function_map["iszcandidate"] = isZcandid;
    function_map["pt"] = Ptof;
    function_map["pz"] = Pzof;
    function_map["nbj"] = nbfof;

    lfunction_map["dr"] = dR;
    lfunction_map["dphi"] = dPhi;
    lfunction_map["deta"] = dEta;

    unfunction_map["hstep"] = hstep;
    unfunction_map["delta"] = delta;
    unfunction_map["anyof"] = abs;
    unfunction_map["allof"] = abs;
    unfunction_map["sqrt"] = sqrt;
    unfunction_map["abs"] = abs;
    unfunction_map["sin"] = sin;
    unfunction_map["cos"] = cos;
    unfunction_map["tan"] = tan;
    unfunction_map["sinh"] = sinh;
    unfunction_map["cosh"] = cosh;
    unfunction_map["tanh"] = tanh;
    unfunction_map["exp"] = exp;
    unfunction_map["log"] = log;
    unfunction_map["not"] = LogicalNot;

    sfunction_map["all"] = all;
    sfunction_map["none"] = none;
    sfunction_map["uweight"] = uweight;
    sfunction_map["lepsf"] = lepsf;
    sfunction_map["btagsf"] = btagsf;
    sfunction_map["xslumicorrsf"] = xslumicorrsf;
    sfunction_map["count"] = count;
    sfunction_map["size"] = count;
    sfunction_map["getindex"] = getIndex;
    sfunction_map["met"] = met;
    sfunction_map["metsig"] = metsig;
    sfunction_map["hlt_iso_mu"] = hlt_iso_mu;
    sfunction_map["hlt_trg"] = hlt_trg;
    sfunction_map["fht"] = ht;

    // razorfunction_map["fmr"] = fMR;
    // razorfunction_map["fmegajets"] = fmegajets;
    // razorfunction_map["fmr"] =
    // razorfunction_map["fmr"] =

  }
} // end namespace adl
