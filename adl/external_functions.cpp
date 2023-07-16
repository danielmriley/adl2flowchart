// Check for external functions for use within ADL file.
#ifndef EXTERNAL_FUNC_CPP
#define EXTERNAL_FUNC_CPP

#include <iostream>
#include <string>
#include <fstream>
#include <cctype>

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

  int check_function_table(std::string id) {
    std::ifstream fin("ext_lib.txt");
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

  int check_property_table(std::string id) {
    std::ifstream fin("property_vars.txt");
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

  int check_object_table(std::string id) {
//    std::string path = ""  // Need to find the dir that the libraries are in.
    std::ifstream fin("./adl/ext_objs.txt");
    std::string input;
    id = toupper(id);

    while(fin >> input) {
      input = toupper(input);
      if(id == input) {
        std::cout << id << " is a predefined OBJECT\n";
        fin.close();
        return 0;
      }
    }
    std::cout << id << " is not a predefined OBJECT\n";
    fin.close();
    return 1;
  }


  // typedef double (*PropFunction)(dbxParticle*);
  // typedef double (*LFunction)(dbxParticle*,dbxParticle*);
  // typedef double (*UnFunction)(double);
  // int set_function_map(std::map<std::string,PropFunction >& function_map,
  //                      std::map<std::string,LFunction >& lfunction_map,
  //                      std::map<std::string,UnFunction >& unfunction_map) {
  //   // std::ifstream fin("BP/property_vars.txt");
  //   // if(!fin.good()) {
  //   //   std::cerr <<"FAILED TO CONNECT TO FILE\n";
  //   //   fin.close();
  //   //   exit(1);
  //   // }
  //   //
  //   // std::string input;
  //   // while(std::getline(fin, input)) {
  //   //   std::stringstream ss(input);
  //   //   std::string property, arrow, func_name;
  //   //   ss >> property; ss >> arrow; ss >> func_name;
  //   //
  //   //   function_map[property] = func_name;
  //   // }
  //   //
  //   // fin.close();
  //
  //   // Map of strings to function pointers.
  //   // Write this in a script to generate the cpp code at compile time.
  //   // Funcs
  //   function_map["mass"] = Mof;
  //   function_map["m"] = Mof;
  //   function_map["mass"] = Mof;
  //   function_map["q"] = Qof;
  //   function_map["charge"] = Qof;
  //   function_map["constituents"] = CCountof;
  //   function_map["daughters"] = CCountof;
  //   function_map["pdgid"] = pdgIDof;
  //   function_map["index"] = IDXof;
  //   function_map["p"] = Pof;
  //   function_map["e"] = Eof;
  //   function_map["tautag"] = isTauTag;
  //   function_map["btag"] = isBTag;
  //   function_map["ctag"] = isBTag;
  //   function_map["btagdeepb"] = DeepBof;
  //   function_map["msoftdrop"] = MsoftDof;
  //   function_map["tau1"] = tau1of;
  //   function_map["tau2"] = tau2of;
  //   function_map["tau3"] = tau3of;
  //   function_map["dxy"] = dxyof;
  //   function_map["edxy"] = edxyof;
  //   function_map["edz"] = edzof;
  //   function_map["dz"] = dzof;
  //   function_map["vertexr"] = vtrof;
  //   function_map["vertexz"] = vzof;
  //   function_map["vertexy"] = vyof;
  //   function_map["vertexx"] = vxof;
  //   function_map["vertext"] = vtrof;
  //   function_map["subjet1btag"] = sub1btagof;
  //   function_map["subjet2btag"] = sub2btagof;
  //   function_map["mvaloose"] = mvalooseof;
  //   function_map["mvatight"] = mvatightof;
  //   function_map["sieie"] = sieieof;
  //   function_map["minipfrelisoall"] = relisoof;
  //   function_map["relisoall"] = relisoallof;
  //   function_map["pfreliso03all"] = pfreliso03allof;
  //   function_map["iddecaymode"] = iddecaymodeof;
  //   function_map["idantieletight"] = idantieletightof;
  //   function_map["idantimutight"] = idantimutightof;
  //   function_map["tightid"] = tightidof;
  //   function_map["puid"] = puidof;
  //   function_map["genpartidx"] = genpartidxof;
  //   function_map["decaymode"] = decaymodeof;
  //   function_map["truthparentid"] = truthParentIDof;
  //   function_map["truthid"] = truthIDof;
  //   function_map["truthmatchprob"] = truthMatchProbof;
  //   function_map["averagemu"] = averageMuof;
  //   function_map["softid"] = softIdof;
  //   function_map["status"] = softIdof;
  //   function_map["dmvanewdm2017v2"] = tauisoof;
  //   function_map["phi"] = Phiof;
  //   function_map["rap"] = Rapof;
  //   function_map["eta"] = Etaof;
  //   function_map["abseta"] = AbsEtaof;
  //   function_map["ptcone"] = PtConeof;
  //   function_map["etcone"] = EtConeof;
  //   function_map["isolationvar"] = IsoVarof;
  //   function_map["miniiso"] = MiniIsoVarof;
  //   function_map["tight"] = isTight;
  //   function_map["medium"] = isMedium;
  //   function_map["loose"] = isLoose;
  //   function_map["iszcandidate"] = isZcandid;
  //   function_map["pt"] = Ptof;
  //   function_map["pz"] = Pzof;
  //   function_map["nbj"] = nbfof;
  //
  //   // LFuncs
  //   lfunction_map["dr"] = dR;
  //   lfunction_map["dphi"] = dPhi;
  //   lfunction_map["deta"] = dEta;
  //
  //   // sfunction_map[]
  //
  //   unfunction_map["hstep"] = hstep;
  //   unfunction_map["delta"] = delta;
  //   unfunction_map["anyof"] = abs;
  //   unfunction_map["allof"] = abs;
  //   unfunction_map["sqrt"] = sqrt;
  //   unfunction_map["abs"] = abs;
  //   unfunction_map["sin"] = sin;
  //   unfunction_map["cos"] = cos;
  //   unfunction_map["tan"] = tan;
  //   unfunction_map["sinh"] = sinh;
  //   unfunction_map["cosh"] = cosh;
  //   unfunction_map["tanh"] = tanh;
  //   unfunction_map["exp"] = exp;
  //   unfunction_map["log"] = log;
  //
  //   return 0;
  // }
  //
  // void set_particle_map(std::map<std::string,std::pair<int,std::string>>& particle_map) {
  //   particle_map["gen"] = std::make_pair(10,"TRUTH");
  //   particle_map["ele"] = std::make_pair(electron_t,"ELE");
  //   particle_map["electron"] = std::make_pair(electron_t,"ELE");
  //   particle_map["muo"] = std::make_pair(muon_t,"MUO");
  //   particle_map["muon"] = std::make_pair(muon_t,"MUO");
  //   particle_map["tau"] = std::make_pair(tau_t,"TAU");
  //   particle_map["trk"] = std::make_pair(track_t,"TRACK");
  //   particle_map["lep"] = std::make_pair(1,"ELE");
  //   particle_map["pho"] = std::make_pair(photon_t,"PHO");
  //   particle_map["photon"] = std::make_pair(8,"PHO");
  //   particle_map["jet"] = std::make_pair(2,"JET");
  //   particle_map["bjet"] = std::make_pair(3,"JET");
  //   particle_map["fjet"] = std::make_pair(9,"FJET");
  //   particle_map["fatjet"] = std::make_pair(9,"FJET");
  //   particle_map["qgjet"] = std::make_pair(4,"QCJET");
  //   particle_map["numet"] = std::make_pair(5,"");
  //   particle_map["metlv"] = std::make_pair(7,"");
  //
  //   // particle_map.insert(std::make_pair("gen", std::make_pair(10,"TRUTH")));
  //   // particle_map.insert(std::make_pair("gen", std::make_pair(10,"TRUTH")));
  //   // particle_map.insert(std::make_pair("ele", std::make_pair(electron_t,"ELE")));
  //   // particle_map.insert(std::make_pair("electron", std::make_pair(electron_t,"ELE")));
  //   // particle_map.insert(std::make_pair("muo", std::make_pair(muon_t,"MUO")));
  //   // particle_map.insert(std::make_pair("muon", std::make_pair(muon_t,"MUO")));
  //   // particle_map.insert(std::make_pair("tau", std::make_pair(tau_t,"TAU")));
  //   // particle_map.insert(std::make_pair("trk", std::make_pair(track_t,"TRACK")));
  //   // particle_map.insert(std::make_pair("lep", std::make_pair(1,"ELE")));
  //   // particle_map.insert(std::make_pair("pho", std::make_pair(photon_t,"PHO")));
  //   // particle_map.insert(std::make_pair("photon", std::make_pair(8,"PHO")));
  //   // particle_map.insert(std::make_pair("jet", std::make_pair(2,"JET")));
  //   // particle_map.insert(std::make_pair("bjet", std::make_pair(3,"JET")));
  //   // particle_map.insert(std::make_pair("fjet", std::make_pair(9,"FJET")));
  //   // particle_map.insert(std::make_pair("fatjet", std::make_pair(9,"FJET")));
  //   // particle_map.insert(std::make_pair("qgjet", std::make_pair(4,"QCJET")));
  //   // particle_map.insert(std::make_pair("numet", std::make_pair(5,"")));
  //   // particle_map.insert(std::make_pair("metlv", std::make_pair(7,"")));
  // }


  // int getParticleType(std::string particle) {
  //   if(particle == "Electron") return 1;
  //   if(particle == "Jet") return 2;
  //   if(particle == "BJET") return 3;
  //   if(particle == "LJET") return 4;
  //   if(particle == "Photon") return 8;
  //   if(particle == "FatJet") return 9;
  //   if(particle == "Truth") return 10;
  //   if(particle == "Tau") return 11;
  //   if(particle == "Muon") return 12;
  //   if(particle == "Trk") return 19;
  //   else return 0;
  // }
} // end namespace adl
#endif
