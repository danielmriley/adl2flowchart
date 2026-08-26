#pragma once

/// Internal analysis types and helpers shared by engine / certify-bridge /
/// model-refine. Not a public header: do not include from tests or CLI.

#include "adl2/analysis/analysis.hpp"
#include "adl2/analysis/encode.hpp"
#include "adl2/analysis/interval.hpp"
#include "adl2/analysis/reconcile.hpp"
#include "adl2/analysis/refute.hpp"
#include "adl2/analysis/report.hpp"
#include "adl2/analysis/witness.hpp"
#include "adl2/axioms/axioms.hpp"
#include "adl2/certify/bundle.hpp"
#include "adl2/certify/certify.hpp"
#include "adl2/formula/formula.hpp"
#include "adl2/interp/eval.hpp"
#include "adl2/interp/event.hpp"
#include "adl2/sema/hir.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/solver/solver.hpp"

#include <chrono>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <string>
#include <utility>
#include <vector>

namespace adl2::analysis {

struct RegionCtx {
  IntervalMap intervals;
  std::vector<std::pair<adl2::solver::AssertName, adl2::formula::Over>> overs;
  std::vector<adl2::formula::Under> unders;
};

struct CertPayload {
  std::vector<std::pair<adl2::solver::AssertName, adl2::formula::QFormula>> asserts;
  adl2::certify::Certificate cert;
  bool whole = true;
};

struct Certified {
  std::optional<bool> flag;
  std::optional<CertPayload> payload;
};

struct CombineAcc {
  bool enabled = false;
  const adl2::sema::Hir* hir = nullptr;
  std::map<adl2::solver::AssertName, CoreItem> origins;
  std::map<adl2::solver::AssertName, adl2::certify::DerivedFact> recon_chains;
  std::vector<adl2::certify::CombineBundle> bundles;
};

struct PredImplies {
  bool a_in_b = false;
  bool b_in_a = false;
  std::optional<CertPayload> a_chain;
  std::optional<CertPayload> b_chain;
};

struct ReconRun {
  std::map<std::string, std::size_t> counts;
  std::vector<std::pair<adl2::solver::AssertName, adl2::formula::QFormula>> facts;
  std::vector<ReconReport> ledger;
  std::vector<ReconNearMissReport> near_misses;
};

RegionCtx build_ctx(const RegionEnc& r);
Presence presence_of(const adl2::sema::Hir& hir, adl2::sema::QuantityId q);
std::optional<adl2::sema::QuantityId> lookup_size(const adl2::sema::Hir& hir,
                                                 adl2::sema::CollectionId coll);
std::vector<WitnessValue> witness_values(const adl2::sema::Hir& hir,
                                         const adl2::solver::Model& model,
                                         const std::set<adl2::sema::QuantityId>& mentioned);
std::vector<WitnessValue> validated_witness_values(
    const adl2::sema::Hir& hir, const adl2::sema::ExtDecls& ext, const adl2::interp::Interp& interp,
    const std::string& json, const adl2::solver::Model& model,
    const std::set<adl2::sema::QuantityId>& mentioned);
void declare_all(adl2::solver::Solver& s, const adl2::sema::Hir& hir, const UnitEnc& unit,
                 const adl2::axioms::AxiomSet& axioms, const std::set<adl2::sema::QuantityId>& extra);
void assert_axioms(adl2::solver::Solver& s, const adl2::axioms::AxiomSet& axioms);
void assert_overs(adl2::solver::Solver& s,
                  const std::vector<std::pair<adl2::solver::AssertName, adl2::formula::Over>>& overs);
void assert_unders(adl2::solver::Solver& s, const std::vector<adl2::formula::Under>& unders);
void note_failure(Report& report, const adl2::solver::SatResult& r);
void file_contradiction(Report& report, std::string msg);
void file_fail_closed(Report& report, std::string msg);
std::unique_ptr<adl2::solver::SubprocessSolver> make_solver(SolverChoice choice, std::string& label);

const char* catalog_assumption(adl2::axioms::AxiomId id);
const char* catalog_statement(adl2::axioms::AxiomId id);
std::string size_label(const adl2::sema::Hir& hir, adl2::sema::QuantityId q);
std::string query_role(const std::string& name, const std::string& sub, const std::string& sup);

Certified certify_named_formulas(
    bool certify, const std::optional<std::vector<adl2::solver::AssertName>>& core,
    const std::vector<std::pair<adl2::solver::AssertName, adl2::formula::QFormula>>& extra,
    const adl2::axioms::AxiomSet* axioms,
    const std::vector<std::pair<adl2::solver::AssertName, adl2::formula::QFormula>>* recon_facts,
    bool keep_payload);
void certify_interval_pair(PairReport& pr, const std::vector<RefutingPart>& parts,
                           const RegionCtx& ca, const RegionCtx& cb, bool certify, bool demote,
                           Report& report, CombineAcc& acc);
void certify_interval_bin(const std::vector<RefutingPart>& parts, const RegionCtx& region_ctx,
                          const adl2::solver::AssertName& bi_name, const adl2::formula::Over& bi,
                          const adl2::solver::AssertName& bj_name, const adl2::formula::Over& bj,
                          bool certify, Report& report, const std::string& region);
void push_bundle(CombineAcc& acc, const std::string& region_a, const std::string& region_b,
                 const CertPayload& payload, Report& report);

PairReport interval_or_solver_pair(
    const adl2::sema::Hir& hir, const adl2::sema::ExtDecls& ext, const adl2::interp::Interp& interp,
    const RegionEnc& ra, const RegionEnc& rb, const RegionCtx& ca, const RegionCtx& cb,
    adl2::solver::Solver* solver, std::chrono::milliseconds timeout, Report& report, bool certify,
    bool demote_uncertified_interval, const adl2::axioms::AxiomSet* axioms,
    const std::vector<std::pair<adl2::solver::AssertName, adl2::formula::QFormula>>* recon_facts,
    CombineAcc& acc);
void gate_pair(PairReport& report, std::size_t ia, std::size_t ib, const adl2::interp::Interp& interp,
               const std::vector<adl2::interp::Event>& gate_events,
               const std::vector<adl2::interp::Event>& refute_probes, Report& diag,
               std::size_t& sample_refutations, std::size_t& refute_refutations);
bool gate_empty(std::size_t idx, const std::string& name, const adl2::interp::Interp& interp,
                const std::vector<adl2::interp::Event>& gate_events, Report& diag,
                std::size_t& refutations);
bool refute_empty(std::size_t idx, const std::string& name, const adl2::interp::Interp& interp,
                  const std::vector<adl2::interp::Event>& refute_probes, Report& diag,
                  std::size_t& refutations);
std::vector<double> cut_constants(const adl2::sema::Hir& hir);
ReconRun apply_reconcile(adl2::sema::Hir& hir, const UnitEnc& unit, adl2::solver::Solver* solver,
                         bool certify, std::chrono::milliseconds timeout, Report& report,
                         const adl2::axioms::AxiomSet& axioms, ReconEnc recon, CombineAcc& acc);
BinCheckReport bin_check(
    adl2::solver::Solver* solver, std::chrono::milliseconds timeout, bool certify, Report& report,
    const adl2::axioms::AxiomSet* axioms,
    const std::vector<std::pair<adl2::solver::AssertName, adl2::formula::QFormula>>* recon_facts,
    const adl2::sema::Hir& hir, const BinSetEnc& set, const RegionCtx& region_ctx,
    std::string region_name);

}  // namespace adl2::analysis
