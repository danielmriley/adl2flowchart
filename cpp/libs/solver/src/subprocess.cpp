#include "adl2/solver/sexp.hpp"
#include "adl2/solver/solver.hpp"

#include <fcntl.h>
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>

#include <algorithm>
#include <cerrno>
#include <chrono>
#include <condition_variable>
#include <cstring>
#include <deque>
#include <functional>
#include <map>
#include <mutex>
#include <sstream>
#include <thread>
#include <utility>

namespace adl2::solver {
namespace {

using adl2::formula::LinAtom;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::sema::QuantityId;
using adl2::sema::Rat;
using Clock = std::chrono::steady_clock;

constexpr auto kWatchdogGrace = std::chrono::seconds(2);
constexpr auto kGetterBudget = std::chrono::seconds(30);

enum class ItemKind { Assert, Raw };

struct Item {
  ItemKind kind = ItemKind::Assert;
  std::string smt;
  std::optional<std::string> internal;
  std::optional<AssertName> user;
};

struct Frame {
  std::vector<Item> items;
};

enum class LastCheck { None, Sat, Unsat, Unknown };

enum class FailKind { Timeout, Dead };

struct Fail {
  FailKind kind = FailKind::Dead;
  std::string reason;
};

void ignore_sigpipe() {
  struct sigaction sa {};
  sa.sa_handler = SIG_IGN;
  sigemptyset(&sa.sa_mask);
  sa.sa_flags = 0;
  ::sigaction(SIGPIPE, &sa, nullptr);
}

void close_fd(int& fd) {
  if (fd >= 0) {
    ::close(fd);
    fd = -1;
  }
}

void read_lines(int fd, const std::function<void(std::string)>& on_line) {
  std::string buf;
  char tmp[4096];
  while (true) {
    ssize_t n = ::read(fd, tmp, sizeof tmp);
    if (n < 0) {
      if (errno == EINTR) continue;
      break;
    }
    if (n == 0) break;
    for (ssize_t i = 0; i < n; ++i) {
      if (tmp[i] == '\n') {
        if (!buf.empty() && buf.back() == '\r') buf.pop_back();
        on_line(buf);
        buf.clear();
      } else {
        buf.push_back(tmp[i]);
      }
    }
  }
  if (!buf.empty()) {
    if (buf.back() == '\r') buf.pop_back();
    on_line(buf);
  }
}

bool write_all(int fd, const char* data, std::size_t len) {
  std::size_t off = 0;
  while (off < len) {
    ssize_t n = ::write(fd, data + off, len - off);
    if (n < 0) {
      if (errno == EINTR) continue;
      return false;
    }
    if (n == 0) return false;
    off += static_cast<std::size_t>(n);
  }
  return true;
}

std::string stderr_tail(const std::string& errs) {
  std::size_t b = 0;
  while (b < errs.size() &&
         (errs[b] == ' ' || errs[b] == '\t' || errs[b] == '\r' ||
          errs[b] == '\n')) {
    ++b;
  }
  std::size_t e = errs.size();
  while (e > b && (errs[e - 1] == ' ' || errs[e - 1] == '\t' ||
                   errs[e - 1] == '\r' || errs[e - 1] == '\n')) {
    --e;
  }
  if (b == e) return "";
  return "; stderr: " + errs.substr(b, e - b);
}

const char* sort_name(QSort sort) {
  return sort == QSort::Int ? "Int" : "Real";
}

std::uint64_t timeout_ms(std::chrono::milliseconds timeout) {
  auto ms = timeout.count();
  if (ms < 1) return 1;
  return static_cast<std::uint64_t>(ms);
}

struct Live {
  pid_t pid = -1;
  int stdin_fd = -1;
  bool reaped = false;
  std::thread out_reader;
  std::thread err_reader;
  std::mutex mu;
  std::condition_variable cv;
  std::deque<std::string> lines;
  bool stdout_eof = false;
  std::mutex stderr_mu;
  std::string stderr_buf;

  Live() = default;
  Live(const Live&) = delete;
  Live& operator=(const Live&) = delete;

  ~Live() { shutdown(); }

  std::string take_stderr() {
    std::lock_guard<std::mutex> g(stderr_mu);
    std::string out;
    std::swap(out, stderr_buf);
    return out;
  }

  void shutdown() {
    if (!reaped && pid > 0) {
      ::kill(pid, SIGKILL);
      int status = 0;
      ::waitpid(pid, &status, 0);
      reaped = true;
    }
    close_fd(stdin_fd);
    if (out_reader.joinable()) out_reader.join();
    if (err_reader.joinable()) err_reader.join();
    pid = -1;
  }
};

std::unique_ptr<Live> spawn_live(const std::string& cmd, std::string& err) {
  ignore_sigpipe();
  int in_p[2] = {-1, -1};
  int out_p[2] = {-1, -1};
  int err_p[2] = {-1, -1};
  int exec_p[2] = {-1, -1};
  auto fail = [&](const std::string& msg) -> std::unique_ptr<Live> {
    err = msg;
    auto cl = [](int* p) {
      close_fd(p[0]);
      close_fd(p[1]);
    };
    cl(in_p);
    cl(out_p);
    cl(err_p);
    cl(exec_p);
    return nullptr;
  };
  auto spawn_fail = [&](const char* why) {
    return fail(std::string(PROCESS_FAILURE) + ": spawn `" + cmd +
                "` failed: " + why);
  };

  if (::pipe(in_p) != 0 || ::pipe(out_p) != 0 || ::pipe(err_p) != 0 ||
      ::pipe(exec_p) != 0) {
    return spawn_fail(std::strerror(errno));
  }
  ::fcntl(exec_p[1], F_SETFD, FD_CLOEXEC);

  pid_t pid = ::fork();
  if (pid < 0) return spawn_fail(std::strerror(errno));
  if (pid == 0) {
    ::dup2(in_p[0], STDIN_FILENO);
    ::dup2(out_p[1], STDOUT_FILENO);
    ::dup2(err_p[1], STDERR_FILENO);
    ::close(in_p[0]);
    ::close(in_p[1]);
    ::close(out_p[0]);
    ::close(out_p[1]);
    ::close(err_p[0]);
    ::close(err_p[1]);
    ::close(exec_p[0]);
    ::execlp(cmd.c_str(), cmd.c_str(), "-in", static_cast<char*>(nullptr));
    int e = errno;
    ssize_t w = ::write(exec_p[1], &e, sizeof e);
    (void)w;
    ::_exit(127);
  }

  ::close(in_p[0]);
  in_p[0] = -1;
  ::close(out_p[1]);
  out_p[1] = -1;
  ::close(err_p[1]);
  err_p[1] = -1;
  ::close(exec_p[1]);
  exec_p[1] = -1;

  int exec_errno = 0;
  ssize_t n = ::read(exec_p[0], &exec_errno, sizeof exec_errno);
  ::close(exec_p[0]);
  exec_p[0] = -1;
  if (n > 0) {
    int status = 0;
    ::waitpid(pid, &status, 0);
    ::close(in_p[1]);
    in_p[1] = -1;
    ::close(out_p[0]);
    out_p[0] = -1;
    ::close(err_p[0]);
    err_p[0] = -1;
    return spawn_fail(std::strerror(exec_errno));
  }

  auto live = std::make_unique<Live>();
  live->pid = pid;
  live->stdin_fd = in_p[1];
  in_p[1] = -1;
  int out_fd = out_p[0];
  out_p[0] = -1;
  int err_fd = err_p[0];
  err_p[0] = -1;
  ::fcntl(live->stdin_fd, F_SETFD, FD_CLOEXEC);
  ::fcntl(out_fd, F_SETFD, FD_CLOEXEC);
  ::fcntl(err_fd, F_SETFD, FD_CLOEXEC);

  Live* raw = live.get();
  raw->out_reader = std::thread([raw, out_fd]() {
    read_lines(out_fd, [raw](std::string line) {
      std::lock_guard<std::mutex> g(raw->mu);
      raw->lines.push_back(std::move(line));
      raw->cv.notify_one();
    });
    ::close(out_fd);
    {
      std::lock_guard<std::mutex> g(raw->mu);
      raw->stdout_eof = true;
    }
    raw->cv.notify_one();
  });
  raw->err_reader = std::thread([raw, err_fd]() {
    read_lines(err_fd, [raw](std::string line) {
      std::lock_guard<std::mutex> g(raw->stderr_mu);
      raw->stderr_buf += line;
      raw->stderr_buf += '\n';
    });
    ::close(err_fd);
  });
  return live;
}

}  // namespace

struct SubprocessSolver::Impl {
  std::string cmd;
  std::map<QuantityId, QSort> decls;
  std::vector<Frame> frames;
  std::uint32_t name_seq = 0;
  LastCheck last = LastCheck::None;
  std::chrono::milliseconds last_timeout{10000};
  std::unique_ptr<Live> child;
  std::uint64_t sync_seq = 0;
  bool incremental_live = false;
  /// Last `check` / `check_unsat` shape. Survives `recycle` so a getter
  /// after a dead incremental UNSAT cannot reset-replay a flat script.
  bool last_unsat_incremental = false;
  std::size_t sent_depth = 0;
  std::vector<std::size_t> sent_counts;
  std::map<QuantityId, QSort> sent_decls;
  std::string last_sent;

  Impl() { frames.emplace_back(); }
  ~Impl() { recycle(); }

  void invalidate_incremental() {
    incremental_live = false;
    sent_depth = 0;
    sent_counts.clear();
    sent_decls.clear();
  }

  void recycle() {
    child.reset();
    invalidate_incremental();
  }

  bool ensure_live(std::string& err) {
    if (!child) {
      child = spawn_live(cmd, err);
      if (!child) return false;
    }
    return true;
  }

  std::string atom_smt(const LinAtom& a) {
    std::vector<std::string> terms;
    terms.reserve(a.terms().size());
    for (const auto& t : a.terms()) {
      const Rat& c = t.first;
      QuantityId q = t.second;
      decls.emplace(q, QSort::Real);
      QSort sort = decls[q];
      std::string var = (sort == QSort::Int)
                            ? "(to_real q" + std::to_string(q.id) + ")"
                            : "q" + std::to_string(q.id);
      if (c.is_one()) {
        terms.push_back(std::move(var));
      } else {
        terms.push_back("(* " + c.smt_real() + " " + var + ")");
      }
    }
    std::string lhs;
    if (terms.empty()) {
      lhs = "0.0";
    } else if (terms.size() == 1) {
      lhs = terms[0];
    } else {
      lhs = "(+";
      for (const auto& t : terms) {
        lhs += " ";
        lhs += t;
      }
      lhs += ")";
    }
    const std::string rhs = a.constant().smt_real();
    switch (a.rel()) {
      case Rel::Lt:
        return "(< " + lhs + " " + rhs + ")";
      case Rel::Le:
        return "(<= " + lhs + " " + rhs + ")";
      case Rel::Gt:
        return "(> " + lhs + " " + rhs + ")";
      case Rel::Ge:
        return "(>= " + lhs + " " + rhs + ")";
      case Rel::Eq:
        return "(= " + lhs + " " + rhs + ")";
      case Rel::Ne:
        return "(not (= " + lhs + " " + rhs + "))";
    }
    return "(= " + lhs + " " + rhs + ")";
  }

  std::string formula_smt(const QFormula& f) {
    switch (f.kind) {
      case QFormula::Kind::True:
        return "true";
      case QFormula::Kind::False:
        return "false";
      case QFormula::Kind::Atom:
        return atom_smt(f.atom);
      case QFormula::Kind::And: {
        if (f.items.empty()) return "true";
        std::string s = "(and";
        for (const auto& p : f.items) {
          s += " ";
          s += formula_smt(p);
        }
        s += ")";
        return s;
      }
      case QFormula::Kind::Or: {
        if (f.items.empty()) return "false";
        std::string s = "(or";
        for (const auto& p : f.items) {
          s += " ";
          s += formula_smt(p);
        }
        s += ")";
        return s;
      }
    }
    return "true";
  }

  std::string script() const {
    std::ostringstream s;
    s << "(set-option :produce-models true)\n";
    s << "(set-option :produce-unsat-cores true)\n";
    for (const auto& kv : decls) {
      s << "(declare-const q" << kv.first.id << " " << sort_name(kv.second)
        << ")\n";
    }
    for (const auto& frame : frames) {
      for (const auto& item : frame.items) {
        if (item.kind == ItemKind::Raw) {
          s << item.smt << "\n";
        } else if (!item.internal) {
          s << "(assert " << item.smt << ")\n";
        } else {
          s << "(assert (! " << item.smt << " :named " << *item.internal
            << "))\n";
        }
      }
    }
    return s.str();
  }

  std::string check_query(std::chrono::milliseconds timeout) const {
    std::ostringstream q;
    q << "(reset)\n";
    q << "(set-option :timeout " << timeout_ms(timeout) << ")\n";
    q << script();
    q << "(check-sat)";
    return q.str();
  }

  static std::string item_line(const Item& item) {
    if (item.kind == ItemKind::Raw) return item.smt;
    if (!item.internal) return "(assert " + item.smt + ")";
    return "(assert (! " + item.smt + " :named " + *item.internal + "))";
  }

  std::string bootstrap_unsat_query(std::chrono::milliseconds timeout) const {
    std::ostringstream q;
    q << "(reset)\n";
    q << "(set-option :timeout " << timeout_ms(timeout) << ")\n";
    q << "(set-option :produce-models true)\n";
    q << "(set-option :produce-unsat-cores true)\n";
    for (const auto& kv : decls) {
      q << "(declare-const q" << kv.first.id << " " << sort_name(kv.second)
        << ")\n";
    }
    for (std::size_t i = 0; i < frames.size(); ++i) {
      if (i > 0) q << "(push)\n";
      for (const auto& item : frames[i].items) q << item_line(item) << "\n";
    }
    q << "(check-sat)";
    return q.str();
  }

  std::string delta_unsat_query(std::chrono::milliseconds timeout,
                                std::size_t& out_depth,
                                std::vector<std::size_t>& out_counts,
                                std::map<QuantityId, QSort>& out_decls) const {
    out_depth = sent_depth;
    out_counts = sent_counts;
    out_decls = decls;
    std::ostringstream q;
    for (const auto& kv : decls) {
      if (sent_decls.find(kv.first) != sent_decls.end()) continue;
      q << "(declare-const q" << kv.first.id << " " << sort_name(kv.second)
        << ")\n";
    }
    while (out_depth > frames.size()) {
      q << "(pop)\n";
      --out_depth;
      if (!out_counts.empty()) out_counts.pop_back();
    }
    for (std::size_t i = 0; i < frames.size(); ++i) {
      if (i >= out_depth) {
        q << "(push)\n";
        ++out_depth;
        out_counts.push_back(0);
      }
      std::size_t already = (i < out_counts.size()) ? out_counts[i] : 0;
      for (std::size_t j = already; j < frames[i].items.size(); ++j) {
        q << item_line(frames[i].items[j]) << "\n";
      }
      if (i < out_counts.size())
        out_counts[i] = frames[i].items.size();
      else
        out_counts.push_back(frames[i].items.size());
    }
    q << "(set-option :timeout " << timeout_ms(timeout) << ")\n";
    q << "(check-sat)";
    return q.str();
  }

  void mark_incremental_synced() {
    incremental_live = true;
    sent_depth = frames.size();
    sent_counts.resize(frames.size());
    for (std::size_t i = 0; i < frames.size(); ++i) {
      sent_counts[i] = frames[i].items.size();
    }
    sent_decls = decls;
  }

  bool send_now(const std::string& cmds) {
    if (!child) {
      invalidate_incremental();
      return false;
    }
    std::string reply;
    Fail fail;
    if (!transact(cmds, kGetterBudget, reply, fail)) {
      recycle();
      return false;
    }
    // Echo arrival is not "applied". An ignored `(error "pop")` would
    // leave the child holding asserts the host already dropped.
    SatResult r = classify(reply);
    if (r.is_solver_error() || r.reason == UNSUPPORTED) {
      recycle();
      return false;
    }
    return true;
  }

  // Ok: reply string. Err: Fail.
  bool transact(const std::string& cmds, std::chrono::milliseconds budget,
                std::string& reply, Fail& fail) {
    ++sync_seq;
    const std::string sentinel = "@@adl-sync-" + std::to_string(sync_seq);
    if (!child) {
      fail.kind = FailKind::Dead;
      fail.reason = std::string(PROCESS_FAILURE) + ": `" + cmd +
                    "` is not running";
      return false;
    }
    Live* live = child.get();
    const std::string framed = cmds + "\n(echo \"" + sentinel + "\")\n";
    if (live->stdin_fd < 0 ||
        !write_all(live->stdin_fd, framed.data(), framed.size())) {
      int saved = errno;
      std::string errs = live->take_stderr();
      fail.kind = FailKind::Dead;
      fail.reason = std::string(PROCESS_FAILURE) + ": write to `" + cmd +
                    "` failed: " + std::strerror(saved) + stderr_tail(errs);
      return false;
    }

    auto deadline = Clock::now() + budget;
    reply.clear();
    for (;;) {
      auto now = Clock::now();
      if (now >= deadline) {
        fail.kind = FailKind::Timeout;
        return false;
      }
      auto left = deadline - now;
      std::unique_lock<std::mutex> lock(live->mu);
      bool ready = live->cv.wait_for(lock, left, [&] {
        return !live->lines.empty() || live->stdout_eof;
      });
      if (!ready) {
        fail.kind = FailKind::Timeout;
        return false;
      }
      if (live->lines.empty() && live->stdout_eof) {
        std::string errs = live->take_stderr();
        lock.unlock();
        fail.kind = FailKind::Dead;
        fail.reason = std::string(PROCESS_FAILURE) + ": `" + cmd +
                      "` died mid-query (EOF on stdout)" + stderr_tail(errs);
        return false;
      }
      std::string line = std::move(live->lines.front());
      live->lines.pop_front();
      lock.unlock();

      auto t = line;
      // trim for sentinel compare
      std::size_t b = 0;
      while (b < t.size() &&
             (t[b] == ' ' || t[b] == '\t' || t[b] == '\r' || t[b] == '\n')) {
        ++b;
      }
      std::size_t e = t.size();
      while (e > b && (t[e - 1] == ' ' || t[e - 1] == '\t' || t[e - 1] == '\r' ||
                       t[e - 1] == '\n')) {
        --e;
      }
      t = t.substr(b, e - b);
      if (t == sentinel) break;
      if (t.size() >= 11 && t.compare(0, 11, "@@adl-sync-") == 0) continue;
      reply += line;
      reply += '\n';
    }
    std::string errs = live->take_stderr();
    std::size_t tb = 0;
    while (tb < errs.size() &&
           (errs[tb] == ' ' || errs[tb] == '\t' || errs[tb] == '\r' ||
            errs[tb] == '\n')) {
      ++tb;
    }
    if (tb < errs.size()) reply += errs;
    return true;
  }

  SatResult finish_check(const std::string& query, std::chrono::milliseconds timeout) {
    last_sent = query;
    std::string reply;
    Fail fail;
    auto budget = timeout + kWatchdogGrace;
    if (transact(query, budget, reply, fail)) {
      SatResult verdict = classify(reply);
      if (verdict.is_unknown() &&
          verdict.reason.compare(0, std::strlen(NO_ANSWER), NO_ANSWER) == 0) {
        recycle();
      }
      return verdict;
    }
    if (fail.kind == FailKind::Timeout) {
      recycle();
      return SatResult::unknown(TIMEOUT);
    }
    recycle();
    return SatResult::unknown(std::move(fail.reason));
  }

  SatResult run_check(std::chrono::milliseconds timeout) {
    std::string err;
    if (!ensure_live(err)) return SatResult::unknown(std::move(err));
    // SAT/model path: always reset so z3 uses the non-incremental tactic.
    invalidate_incremental();
    last_unsat_incremental = false;
    return finish_check(check_query(timeout), timeout);
  }

  SatResult run_unsat_check(std::chrono::milliseconds timeout) {
    std::string err;
    if (!ensure_live(err)) return SatResult::unknown(std::move(err));
    last_unsat_incremental = true;
    if (!incremental_live || sent_depth == 0) {
      SatResult verdict = finish_check(bootstrap_unsat_query(timeout), timeout);
      if (child && (verdict.is_sat() || verdict.is_unsat())) {
        mark_incremental_synced();
      } else {
        recycle();
      }
      return verdict;
    }
    std::size_t new_depth = 0;
    std::vector<std::size_t> new_counts;
    std::map<QuantityId, QSort> new_decls;
    std::string query =
        delta_unsat_query(timeout, new_depth, new_counts, new_decls);
    SatResult verdict = finish_check(query, timeout);
    if (child && (verdict.is_sat() || verdict.is_unsat())) {
      sent_depth = new_depth;
      sent_counts = std::move(new_counts);
      sent_decls = std::move(new_decls);
      incremental_live = true;
    } else {
      recycle();
    }
    return verdict;
  }

  std::optional<std::string> getter(const std::string& gcmd) {
    if (!child) {
      // Incremental UNSAT left a scoped session. A flat `(reset)` replay
      // is a different query; do not rebuild a core/model from it.
      if (last_unsat_incremental) return std::nullopt;
      std::string err;
      if (!ensure_live(err)) return std::nullopt;
      std::string reply;
      Fail fail;
      auto budget = last_timeout + kWatchdogGrace;
      if (!transact(check_query(last_timeout), budget, reply, fail)) {
        recycle();
        return std::nullopt;
      }
      SatResult got = classify(reply);
      bool same =
          (got.is_sat() && last == LastCheck::Sat) ||
          (got.is_unsat() && last == LastCheck::Unsat);
      if (!same) return std::nullopt;
    }
    std::string reply;
    Fail fail;
    if (!transact(gcmd, kGetterBudget, reply, fail)) {
      recycle();
      return std::nullopt;
    }
    if (reply.find("(error") != std::string::npos) return std::nullopt;
    return reply;
  }
};

SubprocessSolver SubprocessSolver::z3() { return SubprocessSolver("z3"); }

SubprocessSolver::SubprocessSolver(std::string cmd)
    : impl_(std::make_unique<Impl>()) {
  impl_->cmd = std::move(cmd);
}

SubprocessSolver::SubprocessSolver(SubprocessSolver&&) noexcept = default;
SubprocessSolver& SubprocessSolver::operator=(SubprocessSolver&&) noexcept =
    default;
SubprocessSolver::~SubprocessSolver() = default;

void SubprocessSolver::declare(QuantityId q, QSort sort) {
  impl_->decls.emplace(q, sort);
  if (impl_->incremental_live &&
      impl_->sent_decls.find(q) == impl_->sent_decls.end()) {
    std::string cmd = std::string("(declare-const q") + std::to_string(q.id) +
                      " " + sort_name(sort) + ")";
    if (!impl_->send_now(cmd)) return;
    impl_->sent_decls.emplace(q, sort);
  }
}

void SubprocessSolver::push() {
  impl_->frames.emplace_back();
  impl_->last = LastCheck::None;
  // Depth-only deltas cannot see pop-then-push (same depth, new frame).
  // While the incremental session is live, emit SMT-LIB scopes immediately.
  if (impl_->incremental_live) {
    if (!impl_->send_now("(push)")) return;
    impl_->sent_depth++;
    impl_->sent_counts.push_back(0);
  }
}

void SubprocessSolver::pop() {
  if (impl_->frames.size() > 1) impl_->frames.pop_back();
  impl_->last = LastCheck::None;
  if (impl_->incremental_live && impl_->sent_depth > 1) {
    if (!impl_->send_now("(pop)")) return;
    impl_->sent_depth--;
    if (!impl_->sent_counts.empty()) impl_->sent_counts.pop_back();
  }
}

void SubprocessSolver::assert_formula(
    const QFormula& f, const std::optional<AssertName>& name) {
  Item item;
  item.kind = ItemKind::Assert;
  item.smt = impl_->formula_smt(f);
  if (name) {
    ++impl_->name_seq;
    item.internal = "n" + std::to_string(impl_->name_seq);
    item.user = *name;
  }
  if (impl_->incremental_live) {
    std::ostringstream decls;
    for (const auto& kv : impl_->decls) {
      if (impl_->sent_decls.find(kv.first) != impl_->sent_decls.end()) continue;
      decls << "(declare-const q" << kv.first.id << " "
            << sort_name(kv.second) << ")\n";
    }
    std::string preamble = decls.str();
    if (!preamble.empty()) {
      if (!impl_->send_now(preamble)) {
        impl_->frames.back().items.push_back(std::move(item));
        impl_->last = LastCheck::None;
        return;
      }
      impl_->sent_decls = impl_->decls;
    }
    if (!impl_->send_now(Impl::item_line(item))) {
      impl_->frames.back().items.push_back(std::move(item));
      impl_->last = LastCheck::None;
      return;
    }
    if (impl_->sent_counts.size() < impl_->frames.size()) {
      impl_->sent_counts.resize(impl_->frames.size(), 0);
    }
    if (!impl_->sent_counts.empty()) impl_->sent_counts.back()++;
  }
  impl_->frames.back().items.push_back(std::move(item));
  impl_->last = LastCheck::None;
}

SatResult SubprocessSolver::check(std::chrono::milliseconds timeout) {
  impl_->last_timeout = timeout;
  SatResult result = impl_->run_check(timeout);
  if (result.is_sat())
    impl_->last = LastCheck::Sat;
  else if (result.is_unsat())
    impl_->last = LastCheck::Unsat;
  else
    impl_->last = LastCheck::Unknown;
  return result;
}

SatResult SubprocessSolver::check_unsat(std::chrono::milliseconds timeout) {
  impl_->last_timeout = timeout;
  SatResult result = impl_->run_unsat_check(timeout);
  if (result.is_sat())
    impl_->last = LastCheck::Sat;
  else if (result.is_unsat())
    impl_->last = LastCheck::Unsat;
  else
    impl_->last = LastCheck::Unknown;
  return result;
}

std::optional<Model> SubprocessSolver::model() {
  if (impl_->last != LastCheck::Sat || impl_->decls.empty()) {
    return std::nullopt;
  }
  std::string names;
  bool first = true;
  for (const auto& kv : impl_->decls) {
    if (!first) names += " ";
    first = false;
    names += "q" + std::to_string(kv.first.id);
  }
  auto reply = impl_->getter("(get-value (" + names + "))");
  if (!reply) return std::nullopt;
  auto parsed = parse_value_list(*reply);
  if (!parsed) return std::nullopt;
  std::map<QuantityId, Rat> map;
  for (const auto& kv : *parsed) {
    if (kv.first.size() < 2 || kv.first[0] != 'q') return std::nullopt;
    try {
      std::size_t idx = 0;
      unsigned long id = std::stoul(kv.first.substr(1), &idx);
      if (idx != kv.first.size() - 1) return std::nullopt;
      map.emplace(QuantityId{static_cast<std::uint32_t>(id)}, kv.second);
    } catch (...) {
      return std::nullopt;
    }
  }
  return Model(std::move(map));
}

std::optional<std::vector<AssertName>> SubprocessSolver::unsat_core() {
  if (impl_->last != LastCheck::Unsat) return std::nullopt;
  auto reply = impl_->getter("(get-unsat-core)");
  if (!reply) return std::nullopt;
  auto internals = parse_symbol_list(*reply);
  if (!internals) return std::nullopt;
  std::map<std::string, AssertName> by_internal;
  for (const auto& frame : impl_->frames) {
    for (const auto& item : frame.items) {
      if (item.kind == ItemKind::Assert && item.internal && item.user) {
        by_internal.emplace(*item.internal, *item.user);
      }
    }
  }
  std::vector<AssertName> names;
  for (const auto& n : *internals) {
    auto it = by_internal.find(n);
    if (it == by_internal.end()) {
      // Stale or unmapped `:named` symbol. A truncated Some would let
      // certify replay a weaker subset and still ship PROVEN. Fail closed.
      return std::nullopt;
    }
    names.push_back(it->second);
  }
  std::sort(names.begin(), names.end());
  names.erase(std::unique(names.begin(), names.end()), names.end());
  return names;
}

const char* SubprocessSolver::backend_name() const {
  return "smtlib-subprocess";
}

void SubprocessSolver::inject_raw(std::string smt) {
  Item item;
  item.kind = ItemKind::Raw;
  item.smt = std::move(smt);
  // Raw SMT is not a frame item z3 will apply the same way as an assert.
  // Smash2 only appends; the next `(reset)` replay makes errors sticky.
  // Sending into a live incremental session is a scope/option oracle.
  if (impl_->incremental_live) impl_->invalidate_incremental();
  impl_->frames.back().items.push_back(std::move(item));
  impl_->last = LastCheck::None;
}

bool SubprocessSolver::kill_child_for_test() {
  if (!impl_->child) return false;
  Live* live = impl_->child.get();
  if (live->pid > 0 && !live->reaped) {
    ::kill(live->pid, SIGKILL);
    int status = 0;
    ::waitpid(live->pid, &status, 0);
    live->reaped = true;
  }
  return true;
}

std::string SubprocessSolver::check_query(
    std::chrono::milliseconds timeout) const {
  return impl_->check_query(timeout);
}

std::string SubprocessSolver::unsat_bootstrap_query(
    std::chrono::milliseconds timeout) const {
  return impl_->bootstrap_unsat_query(timeout);
}

std::string SubprocessSolver::last_query() const { return impl_->last_sent; }

}  // namespace adl2::solver
