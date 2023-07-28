#include "driver.h"

namespace adl {
  std::string toupper(std::string s);

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
    typeTable["CONSTIT"] = consti_t;
  }

  int Driver::visitAST(int (*f)(ExprVector& _ast)) {
    return f(ast);
  }

  void Driver::loadFromLibraries() {
    std::ifstream fin("./adl/ext_objs.txt");
    std::string input;

    while(fin >> input) {
      input = toupper(input);
      addObject(input,std::string("PARENT"));
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
        // std::cout << "ERROR: There is a type mismatch\n";
        return "";
      }
    }
    else return static_cast<VarNode*>(expr)->getType();
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
        if(v == var) {
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
            if(varDeclType != "NOT FOUND" && varDeclType == "PARENT") {
              on->setObjectType(var);
              dependencyChart[var].push_back(on->getId());
            }
            else if(checkObjectTable(var) == 0) { // Here means its a declared type.
              for(auto &p: dependencyChart) {
                auto itr = std::find(p.second.begin(), p.second.end(), var);
                if(itr != p.second.end()) {
                  on->setObjectType(p.first);
                  dependencyChart[p.first].push_back(on->getId());
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

  myParticle* Driver::createParticle(VarNode* vn) {
    myParticle* part = new myParticle;
    part->type = typeTable[toupper(vn->getType())];
    std::cout << "SIZE: " << vn->getAccSize() << std::endl;
    std::vector<int> ind = vn->getAccessor();
    std::cout << "createParticle" << std::endl;
    if(ind.size() > 0) {
      part->index = ind[0]; // DR: temporary. Needs to accomodate range of indecies.
    }
    else {
      part->index = 6213;
    }
    std::cout << "INDEX: " << part->index << std::endl;
    part->collection = vn->getType();

    return part;
  }

  Node* Driver::getFuncNode(Expr* f) {
    Node* node;
    FunctionNode* fn = getFunctionNode(f);
    std::vector<myParticle*> particlesList;
    std::string funcName = fn->getId();
    funcName = tolower(funcName);

    auto funcItr = function_map.find(funcName);
    auto lFuncItr = lfunction_map.find(funcName);
    auto uFuncItr = unfunction_map.find(funcName);
    auto sFuncItr = sfunction_map.find(funcName);

    if(funcItr != function_map.end()) { // Particle attribute
      ExprVector params = fn->getParams();
      for(auto& p : params) {
        VarNode* param = getVarNode(p);
        // Fill particlesList with the particles that are params.
        std::cout << "PARAM: " << param->getId() << "\n";
        std::cout << "TYPE: " << getObjectDeclType(param->getId()) << "\n";
        myParticle* part = createParticle(param);
        particlesList.push_back(part);
      }
      node = new FuncNode(function_map[funcName],particlesList,funcName);
    }
    if(lFuncItr != lfunction_map.end()) {}
    if(uFuncItr != unfunction_map.end()) { // Unary functions
      auto p = fn->getParams();
      node = new UnaryAONode(unfunction_map[funcName], makeNode(p[0]), funcName);
    }

    if(sFuncItr != sfunction_map.end()) { // "special"(?) functions
      if(funcName == "met" || funcName == "all" || funcName == "none") {
        node = new SFuncNode(sfunction_map[funcName], 1, funcName);
      }
      if(funcName == "metsig") {
        node = new SFuncNode(sfunction_map[funcName], 3.1416, funcName);
      }
    }
    return node;
  }

  Node* Driver::makeNode(Expr* expr) {
    // expr should be the RHS of an equal sign.
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
      else if(bn->getOp() == "&&") {
        node = new BinaryNode(LogicalAnd, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
      else if(bn->getOp() == "||") {
        node = new BinaryNode(LogicalOr, makeNode(lhs), makeNode(rhs), bn->getOp());
      }
    }

    if(expr->getToken() == "FUNCTION") {
      std::cout << "Making a FUNCTION NODE\n";
      FunctionNode* fn = getFunctionNode(expr);
      node = getFuncNode(fn);
    }

    if(expr->getToken() == "ID") {
      std::map<std::string, Node*>::iterator it;
      it = NodeVars->find(expr->getId());
      std::map<std::string, Node*>::iterator ito;
      it = ObjectCuts->find(expr->getId());
      if(it != NodeVars->end()) {
        std::cout << "Finding a NODE\n";
        node=it->second;
      }
      else if(ito != ObjectCuts->end()) {
        std::cout << "Found an OBJ NODE\n";
        node = ito->second;
      }
      else if(expr->getId() == "all" || expr->getId() == "none") {
        node = new SFuncNode(sfunction_map[tolower(expr->getId())], 1, expr->getId());
      }
    }

    if(expr->getToken() == "REAL" || expr->getToken() == "INT") {
      std::cout << "Making a VALUE NODE\n";
      node = new ValueNode(expr->value());
    }

    if(expr->getToken() == "ITE") {
      std::cout << "Making a ITE NODE\n";
      ITENode* ite = getITENode(expr);
      node = new IfNode(makeNode(ite->getCondtion()), makeNode(ite->getThenBranch()), makeNode(ite->getElseBranch()), "if");
    }
    std::cout << "\n";
    return node;
  }

  Node* Driver::createParentObject(std::string id) {
    id = toupper(id);
    std::cout << "NAME: " << id << std::endl;;
    Node* node = nullptr;
    std::vector<Node*> newList;
    if(id == "ELE" || id == "ELECTRON") {
      node = new ObjectNode("ELE", NULL, createNewEle, newList, "obj ELE");
    }
    else if(id == "MUO" || id == "MUON") {
      node = new ObjectNode("MUO", NULL, createNewMuo, newList, "obj MUO");
    }
    else if(id == "PHO" || id == "PHOTON") {
      node = new ObjectNode("PHO", NULL, createNewPho, newList, "obj PHO");
    }
    else if(id == "TRK" || id == "TRACK") {
      node = new ObjectNode("Track", NULL, createNewTrack, newList, "obj TRACK");
    }
    else if(id == "FJET" || id == "FATJET") {
      node = new ObjectNode("FJET", NULL, createNewFJet, newList, "obj FatJet");
    }
    else if(id == "TAU") {
      node = new ObjectNode("TAU", NULL, createNewTau, newList, "obj TAU");
    }
    else if(id == "TRUTH") {
      node = new ObjectNode("Truth", NULL, createNewTruth, newList, "obj Truth");
    }
    else if(id == "JET") {
      node = new ObjectNode("JET", NULL, createNewJet, newList, "obj JET");
    }
    return node;
  }

  void Driver::processObject(astObjectNode* on) {
    ExprVector stmnts = on->getStatements();

    Node* parent = nullptr;
    Node* obj = nullptr;
    std::vector<Node*> newList;
    for(auto& s: stmnts) {
      if(s->getToken() == "TAKE") {
        CommandNode* cn = getCommandNode(s);
        Expr* cond = cn->getCondition();
        if(cond->getToken() == "ID") {
          VarNode* vn = getVarNode(cond);
          std::string type = getObjectDeclType(vn->getId());
          if(type == "PARENT") {
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
    //  if(cn->getToken() == "SELECT") {
        std::cout << "HERE" << std::endl;
        NodeCuts->insert(std::make_pair(++cutcount, makeNode(cn->getCondition())));
      //}
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
        pnum = 0;
        parts->push_back(name + " : " + "");

        if(varNode->getType() == "REAL") {
          // Make a node out of the RHS of the = or :
          Node* node;
          node = makeNode(dn->getBody());
          NodeVars->insert(std::make_pair(name, node));
        }
        else { // Means it's a particle definition.
          std::cout << "*** ADDING TO LISTPARTS ***\n";
          std::vector<myParticle*> particles;
          gatherParticles(dn->getBody(), particles);
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
    sfunction_map["getIndex"] = getIndex;
    sfunction_map["met"] = met;
    sfunction_map["metsig"] = metsig;
    sfunction_map["hlt_iso_mu"] = hlt_iso_mu;
    sfunction_map["hlt_trg"] = hlt_trg;
    sfunction_map["ht"] = ht;

  }
} // end namespace adl
