

#include "cutlang_declares.h"

namespace adl {

  // Map of strings to function pointers.
  // Write this in a script to generate the cpp code at compile time.
  // std::map<std::string, PropFunction> function_map;
  // std::map<std::string, LFunction> lfunction_map;
  // std::map<std::string, UnFunction> unfunction_map;

  double      specialf( dbxParticle* apart) { return 0.0; }
  double      Qof( dbxParticle* apart) { return 0.0; }
  double      Mof( dbxParticle* apart) { return 0.0; }
  double      Eof( dbxParticle* apart) { return 0.0; }
  double      Pof( dbxParticle* apart) { return 0.0; }
  double     Pzof( dbxParticle* apart) { return 0.0; }
  double     Ptof( dbxParticle* apart) { return 0.0; }
  double PtConeof( dbxParticle* apart) { return 0.0; }
  double EtConeof( dbxParticle* apart) { return 0.0; }
  double AbsEtaof( dbxParticle* apart) { return 0.0; }
  double    Etaof( dbxParticle* apart) { return 0.0; }
  double    Rapof( dbxParticle* apart) { return 0.0; }
  double    Phiof( dbxParticle* apart) { return 0.0; }
  double 	pdgIDof( dbxParticle* apart) { return 0.0; }
  double flavorof( dbxParticle* apart) { return 0.0; }
  double MsoftDof( dbxParticle* apart) { return 0.0; }
  double  DeepBof( dbxParticle* apart) { return 0.0; }
  double   isBTag( dbxParticle* apart) { return 0.0; }
  double isTauTag( dbxParticle* apart) { return 0.0; }
  double isTight ( dbxParticle* apart) { return 0.0; }
  double isMedium( dbxParticle* apart) { return 0.0; }
  double isLoose ( dbxParticle* apart) { return 0.0; }
  double   tau1of( dbxParticle* apart) { return 0.0; }
  double   tau2of( dbxParticle* apart) { return 0.0; }
  double   tau3of( dbxParticle* apart) { return 0.0; }
  double    dxyof( dbxParticle* apart) { return 0.0; }
  double   edxyof( dbxParticle* apart) { return 0.0; }
  double     dzof( dbxParticle* apart) { return 0.0; }
  double    edzof( dbxParticle* apart) { return 0.0; }
  double     vxof( dbxParticle* apart) { return 0.0; }
  double     vyof( dbxParticle* apart) { return 0.0; }
  double     vzof( dbxParticle* apart) { return 0.0; }
  double     vtof( dbxParticle* apart) { return 0.0; }
  double    vtrof( dbxParticle* apart) { return 0.0; }
  double  sieieof( dbxParticle* apart) { return 0.0; }
  double sub1btagof( dbxParticle* apart) { return 0.0; }
  double sub2btagof( dbxParticle* apart) { return 0.0; }
  double mvalooseof( dbxParticle* apart) { return 0.0; }
  double mvatightof( dbxParticle* apart) { return 0.0; }
  double relisoof( dbxParticle* apart) { return 0.0; }
  double isZcandid ( dbxParticle* apart) { return 0.0; }
  double relisoallof( dbxParticle* apart) { return 0.0; }
  double pfreliso03allof( dbxParticle* apart) { return 0.0; }
  double iddecaymodeof( dbxParticle* apart) { return 0.0; }
  double idisotightof( dbxParticle* apart) { return 0.0; }
  double idantieletightof( dbxParticle* apart) { return 0.0; }
  double idantimutightof( dbxParticle* apart) { return 0.0; }
  double tightidof( dbxParticle* apart) { return 0.0; }
  double    puidof( dbxParticle* apart) { return 0.0; }
  double genpartidxof( dbxParticle* apart) { return 0.0; }
  double decaymodeof( dbxParticle* apart) { return 0.0; }
  double tauisoof( dbxParticle* apart) { return 0.0; }
  double softIdof( dbxParticle* apart) { return 0.0; }
  double CCountof( dbxParticle* apart) { return 0.0; }
  double    nbfof( dbxParticle* apart) { return 0.0; }
  double IsoVarof( dbxParticle* apart) { return 0.0; }
  double MiniIsoVarof( dbxParticle* apart) { return 0.0; }
  double averageMuof( dbxParticle* apart) { return 0.0; }
  double truthMatchProbof( dbxParticle* apart) { return 0.0; }
  double truthIDof( dbxParticle* apart) { return 0.0; }
  double truthParentIDof( dbxParticle* apart) { return 0.0; }
  double IDXof( dbxParticle* apart) { return 0.0; }

  // LFuncs
  double dR  (dbxParticle* apart,dbxParticle* apart2) { return 0.0; }
  double dPhi(dbxParticle* apart,dbxParticle* apart2) { return 0.0; }
  double dEta(dbxParticle* apart,dbxParticle* apart2) { return 0.0; }

  // sfunction_map[]
  double hstep(double x) { return 0.0; }
  double delta(double x) { return 0.0; }
  double abs(double x) { return 0.0; }
  double sqrt(double x) { return 0.0; }
  double sin(double x) { return 0.0; }
  double cos(double x) { return 0.0; }
  double tan(double x) { return 0.0; }
  double sinh(double x) { return 0.0; }
  double cosh(double x) { return 0.0; }
  double tanh(double x) { return 0.0; }
  double exp(double x) { return 0.0; }
  double log(double x) { return 0.0; }

  double none(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double all(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double uweight(AnalysisObjects* ao, std::string s, float value) { return 0.0; }
  double lepsf(AnalysisObjects* ao, std::string s, float value) { return 0.0; }
  double btagsf(AnalysisObjects* ao, std::string s, float value) { return 0.0; }
  double xslumicorrsf(AnalysisObjects* ao, std::string s, float value) { return 0.0; }
  double count(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double getIndex(AnalysisObjects* ao, std::string s, float id) { return 0.0; } // new internal function
  double met   (AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double metsig(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double hlt_iso_mu(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double hlt_trg(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double ht(AnalysisObjects* ao, std::string s, float id) { return 0.0; }

  // BinaryNode functions
  double add(double left, double right) { return left + right; }
  double mult(double left, double right) { return left - right; }
  double sub(double left, double right) { return left * right; }
  double div(double left, double right) { return left / right; }

  //double power ALREADY EXIST
  double lt(double left, double right) { return 0.0; }
  double le(double left, double right) { return 0.0; }
  double ge(double left, double right) { return 0.0; }
  double gt(double left, double right) { return 0.0; }
  double eq(double left, double right) { return 0.0; }
  double ne(double left, double right) { return 0.0; }
  double LogicalAnd(double left, double right) { return 0.0; }
  double LogicalOr(double left, double right) { return 0.0; }
  double mnof(double left, double right) { return 0.0; }
  double mxof(double left, double right) { return 0.0; }

  double unaryMinu(double left) { return 0.0; }
  double LogicalNot(double condition) { return 0.0; }

  // Object creation functions
  void updateParticles( Node* cutIt, std::vector<myParticle *>* particles, int ipart, std::string name) {}
  int getCollectionSize(int t2, std::string base_collection2, AnalysisObjects *ao ) { return 1; }
  void createNewJet   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewFJet  (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewEle   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewMuo   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewTau   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewPho   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewCombo (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewParti (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewTruth (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewTrack (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}


  void fillFuncMaps(std::map<std::string, PropFunction> &function_map,
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
}
