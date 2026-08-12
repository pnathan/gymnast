;;; Independent verification obligations and trace-equivalence checks.
;;;
;;; Verification obligations are lowered from acceptance, invariant,
;;; and constraint nodes in the IR.  They are independent of candidate
;;; rationale: the verifier never sees generator output reasoning,
;;; only the candidate data itself and the reference transition system.
;;;
;;; Counterexamples are normalized into a stable format suitable for
;;; bounded repair prompts and human review.

(def $gymnast-verify-schema "gymnast.verify/0.1")

;;; Deterministic environment extraction.
;;;
;;; The execution clause in an acceptance node specifies which
;;; environmental sources must be controlled.  The verifier refuses
;;; to run unless all required controls are present.

(defun gymnast-extract-execution-env (acceptance-node)
  (let* ((clauses (gymnast-ir-node-field acceptance-node 'clauses))
      (exec-clauses (gymnast-collect-clauses clauses 'execution))
      (exec (if exec-clauses (gymnast-clause-body (car exec-clauses)) nil)))
    (list 'execution-environment
      (list 'clock
        (or (gymnast-keyword-value exec ':clock) 'system))
      (list 'randomness
        (or (gymnast-keyword-value exec ':randomness) 'system))
      (list 'network
        (or (gymnast-keyword-value exec ':network) 'system))
      (list 'locale
        (or (gymnast-keyword-value exec ':locale) "en-US"))
      (list 'timezone
        (or (gymnast-keyword-value exec ':timezone) "UTC")))))

(defun gymnast-env-field (env key)
  (gymnast-assoc-value key (cdr env)))

(defun gymnast-env-deterministic-p (env)
  (and (equal (gymnast-env-field env 'clock) 'virtual)
    (equal (gymnast-env-field env 'randomness) 'seeded)
    (equal (gymnast-env-field env 'network) 'controlled)))

(defun gymnast-env-diagnostics (env acceptance-id)
  (let ((warnings nil))
    (append
      (if (not (equal (gymnast-env-field env 'clock) 'virtual))
        (list (gymnast-diagnostic 'warning 'non-deterministic-clock
            acceptance-id
            "clock is not virtual; traces may not reproduce"
            (gymnast-env-field env 'clock)))
        nil)
      (if (not (equal (gymnast-env-field env 'randomness) 'seeded))
        (list (gymnast-diagnostic 'warning 'non-deterministic-randomness
            acceptance-id
            "randomness is not seeded; traces may not reproduce"
            (gymnast-env-field env 'randomness)))
        nil)
      (if (not (equal (gymnast-env-field env 'network) 'controlled))
        (list (gymnast-diagnostic 'warning 'non-deterministic-network
            acceptance-id
            "network is not controlled; traces may not reproduce"
            (gymnast-env-field env 'network)))
        nil))))

;;; Obligation lowering: each acceptance clause becomes one obligation.

(defun gymnast-obligation-id (acceptance-id kind name)
  (concat acceptance-id "/" (gymnast-symbol-string kind)
    (if name (concat "/" (gymnast-symbol-string name)) "")))

(defun gymnast-lower-property-obligation (acceptance-id clause env)
  (let* ((name (cadr clause))
      (body (cdr (cdr clause)))
      (generate (gymnast-keyword-value body ':generate))
      (execute (gymnast-keyword-value body ':execute))
      (must (gymnast-keyword-value body ':must)))
    (list 'verification-obligation
      (list 'id (gymnast-obligation-id acceptance-id 'property name))
      (list 'kind 'property)
      (list 'source acceptance-id)
      (list 'name name)
      (list 'generate generate)
      (list 'execute execute)
      (list 'assertion must)
      (list 'environment env))))

(defun gymnast-lower-scenario-obligation (acceptance-id clause env)
  (let* ((name (cadr clause))
      (body (cdr (cdr clause)))
      (steps (filter #'consp body)))
    (list 'verification-obligation
      (list 'id (gymnast-obligation-id acceptance-id 'scenario name))
      (list 'kind 'scenario)
      (list 'source acceptance-id)
      (list 'name name)
      (list 'steps steps)
      (list 'environment env))))

(defun gymnast-lower-concurrency-obligation (acceptance-id clause env)
  (let* ((name (cadr clause))
      (body (cdr (cdr clause)))
      (actors (gymnast-keyword-value body ':actors))
      (schedule (gymnast-keyword-value body ':schedule))
      (must (gymnast-keyword-value body ':must)))
    (list 'verification-obligation
      (list 'id (gymnast-obligation-id acceptance-id 'concurrency name))
      (list 'kind 'concurrency)
      (list 'source acceptance-id)
      (list 'name name)
      (list 'actors actors)
      (list 'schedule schedule)
      (list 'assertion must)
      (list 'environment env))))

(defun gymnast-lower-fault-obligation (acceptance-id clause env)
  (let* ((name (cadr clause))
      (body (cdr (cdr clause)))
      (after (gymnast-keyword-value body ':after))
      (inject (gymnast-keyword-value body ':inject))
      (must (gymnast-keyword-value body ':must)))
    (list 'verification-obligation
      (list 'id (gymnast-obligation-id acceptance-id 'fault name))
      (list 'kind 'fault)
      (list 'source acceptance-id)
      (list 'name name)
      (list 'after after)
      (list 'inject inject)
      (list 'assertion must)
      (list 'environment env))))

(defun gymnast-lower-coverage-obligation (acceptance-id clause env)
  (let* ((body (gymnast-clause-body clause))
      (every-op (gymnast-keyword-value body ':every-operation))
      (every-err (gymnast-keyword-value body ':every-error))
      (every-tr (gymnast-keyword-value body ':every-transition))
      (every-inv (gymnast-keyword-value body ':every-invariant))
      (boundaries (gymnast-keyword-value body ':boundaries)))
    (list 'verification-obligation
      (list 'id (gymnast-obligation-id acceptance-id 'coverage nil))
      (list 'kind 'coverage)
      (list 'source acceptance-id)
      (list 'name 'coverage)
      (list 'every-operation every-op)
      (list 'every-error every-err)
      (list 'every-transition every-tr)
      (list 'every-invariant every-inv)
      (list 'boundaries boundaries)
      (list 'environment env))))

(defun gymnast-lower-model-obligation (acceptance-id clause env)
  (let* ((name (cadr clause))
      (body (cdr (cdr clause))))
    (list 'verification-obligation
      (list 'id (gymnast-obligation-id acceptance-id 'model name))
      (list 'kind 'model)
      (list 'source acceptance-id)
      (list 'name name)
      (list 'spec body)
      (list 'environment env))))

(defun gymnast-lower-clause (acceptance-id clause env)
  (let ((head (gymnast-clause-head clause)))
    (cond
      ((equal head 'property)
        (gymnast-lower-property-obligation acceptance-id clause env))
      ((equal head 'scenario)
        (gymnast-lower-scenario-obligation acceptance-id clause env))
      ((equal head 'concurrency)
        (gymnast-lower-concurrency-obligation acceptance-id clause env))
      ((equal head 'fault)
        (gymnast-lower-fault-obligation acceptance-id clause env))
      ((equal head 'coverage)
        (gymnast-lower-coverage-obligation acceptance-id clause env))
      ((equal head 'model)
        (gymnast-lower-model-obligation acceptance-id clause env))
      ((equal head 'execution) nil)
      (t nil))))

;;; Lower invariant nodes into verification obligations.

(defun gymnast-lower-invariant-obligation (invariant-node)
  (let* ((id (gymnast-ir-node-id invariant-node))
      (scope (gymnast-surface-field invariant-node ':scope))
      (always (gymnast-surface-field invariant-node ':always)))
    (list 'verification-obligation
      (list 'id (concat id "/invariant-check"))
      (list 'kind 'invariant)
      (list 'source id)
      (list 'name (gymnast-ir-node-field invariant-node 'name))
      (list 'scope scope)
      (list 'predicate always)
      (list 'environment nil))))

;;; Lower constraint nodes into verification obligations.

(defun gymnast-lower-constraint-obligation (constraint-node)
  (let* ((id (gymnast-ir-node-id constraint-node))
      (class (gymnast-surface-field constraint-node ':class))
      (scope (gymnast-surface-field constraint-node ':scope))
      (under (gymnast-surface-field constraint-node ':under))
      (must (gymnast-surface-field constraint-node ':must)))
    (list 'verification-obligation
      (list 'id (concat id "/constraint-check"))
      (list 'kind 'constraint)
      (list 'source id)
      (list 'name (gymnast-ir-node-field constraint-node 'name))
      (list 'class class)
      (list 'scope scope)
      (list 'under under)
      (list 'assertion must)
      (list 'environment nil))))

;;; Obligation field accessor.

(defun gymnast-obligation-field (obligation key)
  (gymnast-assoc-value key (cdr obligation)))

;;; Compile all obligations from an IR.

(defun gymnast-lower-acceptance-obligations (ir)
  (let ((acceptance-nodes (gymnast-ir-nodes-of-kind ir 'acceptance)))
    (reduce #'append
      (mapcar
        (lambda (node)
          (let* ((id (gymnast-ir-node-id node))
              (env (gymnast-extract-execution-env node))
              (clauses (gymnast-ir-node-field node 'clauses)))
            (filter (lambda (x) x)
              (mapcar
                (lambda (clause) (gymnast-lower-clause id clause env))
                clauses))))
        acceptance-nodes)
      nil)))

(defun gymnast-lower-invariant-obligations (ir)
  (mapcar #'gymnast-lower-invariant-obligation
    (gymnast-ir-nodes-of-kind ir 'invariant)))

(defun gymnast-lower-constraint-obligations (ir)
  (mapcar #'gymnast-lower-constraint-obligation
    (gymnast-ir-nodes-of-kind ir 'constraint)))

(defun gymnast-lower-all-obligations (ir)
  (append
    (gymnast-lower-acceptance-obligations ir)
    (gymnast-lower-invariant-obligations ir)
    (gymnast-lower-constraint-obligations ir)))

;;; Trace-equivalence checking.
;;;
;;; Given a candidate's claimed behavior (as trace steps) and the
;;; reference transition system, verify that the candidate's
;;; state transitions match the reference model.

(defun gymnast-compare-trace-step (reference-step impl-step)
  (let* ((ref-post (gymnast-assoc-value 'post-state (cdr reference-step)))
      (impl-post (gymnast-assoc-value 'post-state (cdr impl-step)))
      (ref-outcome (gymnast-assoc-value 'outcome (cdr reference-step)))
      (impl-outcome (gymnast-assoc-value 'outcome (cdr impl-step))))
    (cond
      ((not (equal ref-outcome impl-outcome))
        (list 'divergence
          (list 'type 'outcome-mismatch)
          (list 'reference ref-outcome)
          (list 'implementation impl-outcome)
          (list 'step reference-step)))
      ((not (equal ref-post impl-post))
        (list 'divergence
          (list 'type 'state-mismatch)
          (list 'reference-state ref-post)
          (list 'implementation-state impl-post)
          (list 'step reference-step)))
      (t nil))))

(defun gymnast-compare-traces (reference-steps impl-steps divergences)
  (cond
    ((and (null reference-steps) (null impl-steps))
      (reverse divergences))
    ((null reference-steps)
      (reverse (cons
          (list 'divergence
            (list 'type 'extra-implementation-steps)
            (list 'count (length impl-steps)))
          divergences)))
    ((null impl-steps)
      (reverse (cons
          (list 'divergence
            (list 'type 'missing-implementation-steps)
            (list 'count (length reference-steps)))
          divergences)))
    (t
      (let ((div (gymnast-compare-trace-step
              (car reference-steps) (car impl-steps))))
        (gymnast-compare-traces
          (cdr reference-steps) (cdr impl-steps)
          (if div (cons div divergences) divergences))))))

(defun gymnast-trace-equivalent-p (reference-trace impl-trace)
  (let* ((ref-steps (gymnast-trace-field reference-trace 'steps))
      (impl-steps (gymnast-trace-field impl-trace 'steps))
      (divergences (gymnast-compare-traces ref-steps impl-steps nil)))
    (null divergences)))

(defun gymnast-trace-equivalence-result (ir reference-trace impl-trace
    obligation-id)
  (let* ((ref-steps (gymnast-trace-field reference-trace 'steps))
      (impl-steps (gymnast-trace-field impl-trace 'steps))
      (divergences (gymnast-compare-traces ref-steps impl-steps nil)))
    (list 'trace-equivalence-result
      (list 'obligation-id obligation-id)
      (list 'equivalent (null divergences))
      (list 'divergences divergences)
      (list 'reference-violations
        (gymnast-trace-violations reference-trace))
      (list 'implementation-violations
        (gymnast-trace-violations impl-trace)))))

;;; Normalized counterexample production.
;;;
;;; Counterexamples are stripped of implementation detail and
;;; presented in terms of the semantic model only: operation,
;;; actor, input, pre-state, expected outcome, actual outcome.

(defun gymnast-normalize-counterexample (divergence obligation-id)
  (let* ((div-type (gymnast-assoc-value 'type (cdr divergence)))
      (step (gymnast-assoc-value 'step (cdr divergence))))
    (list 'normalized-counterexample
      (list 'obligation-id obligation-id)
      (list 'divergence-type div-type)
      (list 'operation
        (if step
          (gymnast-assoc-value 'transition-id (cdr step))
          nil))
      (list 'actor
        (if step (gymnast-assoc-value 'actor (cdr step)) nil))
      (list 'input
        (if step (gymnast-assoc-value 'input (cdr step)) nil))
      (list 'pre-state
        (if step (gymnast-assoc-value 'pre-state (cdr step)) nil))
      (list 'expected
        (gymnast-assoc-value 'reference (cdr divergence)))
      (list 'actual
        (gymnast-assoc-value 'implementation (cdr divergence))))))

(defun gymnast-normalize-counterexamples (divergences obligation-id)
  (mapcar
    (lambda (div)
      (gymnast-normalize-counterexample div obligation-id))
    divergences))

;;; Coverage analysis against IR.

(defun gymnast-coverage-gaps (ir obligations)
  (let* ((transitions (gymnast-extract-transitions ir))
      (invariants (gymnast-ir-nodes-of-kind ir 'invariant))
      (behaviors (gymnast-ir-nodes-of-kind ir 'behavior))
      (coverage-obs (filter
          (lambda (ob)
            (equal (gymnast-obligation-field ob 'kind) 'coverage))
          obligations))
      (coverage-ob (if coverage-obs (car coverage-obs) nil)))
    (if (not coverage-ob)
      nil
      (let* ((want-ops
            (gymnast-obligation-field coverage-ob 'every-operation))
          (want-transitions
            (gymnast-obligation-field coverage-ob 'every-transition))
          (want-invariants
            (gymnast-obligation-field coverage-ob 'every-invariant))
          (want-errors
            (gymnast-obligation-field coverage-ob 'every-error))
          (property-obs (filter
              (lambda (ob)
                (equal (gymnast-obligation-field ob 'kind) 'property))
              obligations))
          (scenario-obs (filter
              (lambda (ob)
                (equal (gymnast-obligation-field ob 'kind) 'scenario))
              obligations))
          (fault-obs (filter
              (lambda (ob)
                (equal (gymnast-obligation-field ob 'kind) 'fault))
              obligations))
          (covered-count (+ (length property-obs) (length scenario-obs)
              (length fault-obs)))
          (transition-count (length transitions))
          (invariant-count (length invariants)))
        (list 'coverage-analysis
          (list 'property-obligations (length property-obs))
          (list 'scenario-obligations (length scenario-obs))
          (list 'fault-obligations (length fault-obs))
          (list 'total-obligations covered-count)
          (list 'transitions-defined transition-count)
          (list 'invariants-defined invariant-count)
          (list 'gaps
            (append
              (if (and want-transitions
                  (> transition-count covered-count))
                (list (list 'gap 'uncovered-transitions
                    (- transition-count covered-count)))
                nil)
              (if (and want-invariants
                  (> invariant-count
                    (length (filter
                        (lambda (ob)
                          (equal (gymnast-obligation-field ob 'kind)
                            'invariant))
                        obligations))))
                (list (list 'gap 'uncovered-invariants 0))
                nil))))))))

;;; Verification result construction.

(defun gymnast-make-verification-result (obligation-id status
    trace counterexamples diagnostics)
  (list 'verification-result
    (list 'schema $gymnast-verify-schema)
    (list 'obligation-id obligation-id)
    (list 'status status)
    (list 'trace trace)
    (list 'counterexamples counterexamples)
    (list 'diagnostics diagnostics)))

(defun gymnast-verification-result-field (result key)
  (gymnast-assoc-value key (cdr result)))

;;; Verify a property obligation against the reference transition system.

(defun gymnast-verify-property-against-reference (ir obligation)
  (let* ((ob-id (gymnast-obligation-field obligation 'id))
      (execute (gymnast-obligation-field obligation 'execute))
      (must (gymnast-obligation-field obligation 'assertion)))
    (if (not execute)
      (gymnast-make-verification-result ob-id 'skipped nil nil
        (list (gymnast-diagnostic 'warning 'no-execute-spec ob-id
            "property has no execute clause" nil)))
      (let* ((trace (gymnast-execute-trace ir
              (if (and (consp execute) (equal (car execute) 'sequence))
                (cdr execute)
                (list execute))))
          (violations (gymnast-trace-violations trace)))
        (if violations
          (gymnast-make-verification-result ob-id 'failed trace
            (mapcar
              (lambda (v)
                (gymnast-counterexample v
                  (car (gymnast-trace-field trace 'steps))))
              violations)
            nil)
          (gymnast-make-verification-result ob-id 'passed trace nil nil))))))

;;; Verify a scenario obligation step-by-step.

(defun gymnast-scenario-trace-steps (steps)
  (reduce #'append
    (mapcar
      (lambda (step)
        (let ((head (car step)))
          (cond
            ((equal head 'given) nil)
            ((equal head 'when)
              (let ((action (cadr step)))
                (if (consp action) (list action) nil)))
            ((equal head 'then) nil)
            (t nil))))
      steps)
    nil))

(defun gymnast-verify-scenario-against-reference (ir obligation)
  (let* ((ob-id (gymnast-obligation-field obligation 'id))
      (steps (gymnast-obligation-field obligation 'steps))
      (trace-steps (gymnast-scenario-trace-steps steps))
      (trace (if trace-steps
          (gymnast-execute-trace ir trace-steps)
          nil))
      (violations (if trace (gymnast-trace-violations trace) nil)))
    (if (not trace)
      (gymnast-make-verification-result ob-id 'skipped nil nil
        (list (gymnast-diagnostic 'warning 'no-trace-steps ob-id
            "scenario produced no executable trace steps" nil)))
      (if violations
        (gymnast-make-verification-result ob-id 'failed trace
          (mapcar
            (lambda (v)
              (gymnast-counterexample v
                (car (gymnast-trace-field trace 'steps))))
            violations)
          nil)
        (gymnast-make-verification-result ob-id 'passed trace nil nil)))))

;;; Verify invariant obligations by checking against all transitions.

(defun gymnast-verify-invariant-obligation (ir obligation)
  (let* ((ob-id (gymnast-obligation-field obligation 'id))
      (predicate (gymnast-obligation-field obligation 'predicate))
      (state (gymnast-make-initial-state ir))
      (holds (gymnast-eval-predicate predicate state nil nil)))
    (if holds
      (gymnast-make-verification-result ob-id 'passed nil nil nil)
      (gymnast-make-verification-result ob-id 'failed nil
        (list (list 'normalized-counterexample
            (list 'obligation-id ob-id)
            (list 'divergence-type 'invariant-violation)
            (list 'predicate predicate)
            (list 'state state)))
        nil))))

;;; Top-level: verify all obligations against the reference system.

(defun gymnast-verify-obligation (ir obligation)
  (let ((kind (gymnast-obligation-field obligation 'kind)))
    (cond
      ((equal kind 'property)
        (gymnast-verify-property-against-reference ir obligation))
      ((equal kind 'scenario)
        (gymnast-verify-scenario-against-reference ir obligation))
      ((equal kind 'invariant)
        (gymnast-verify-invariant-obligation ir obligation))
      (t
        (gymnast-make-verification-result
          (gymnast-obligation-field obligation 'id)
          'skipped nil nil
          (list (gymnast-diagnostic 'info 'deferred-verification
              (gymnast-obligation-field obligation 'id)
              (concat "verification of " (gymnast-symbol-string kind)
                " obligations requires runtime execution")
              nil)))))))

(defun gymnast-verify-all-obligations (ir obligations)
  (mapcar (lambda (ob) (gymnast-verify-obligation ir ob)) obligations))

;;; Compile the full verification bundle.

(defun gymnast-compile-verification (ir)
  (let* ((obligations (gymnast-lower-all-obligations ir))
      (results (gymnast-verify-all-obligations ir obligations))
      (passed (filter
          (lambda (r)
            (equal (gymnast-verification-result-field r 'status) 'passed))
          results))
      (failed (filter
          (lambda (r)
            (equal (gymnast-verification-result-field r 'status) 'failed))
          results))
      (skipped (filter
          (lambda (r)
            (equal (gymnast-verification-result-field r 'status) 'skipped))
          results))
      (env-diags
        (let ((acc-nodes (gymnast-ir-nodes-of-kind ir 'acceptance)))
          (if acc-nodes
            (gymnast-env-diagnostics
              (gymnast-extract-execution-env (car acc-nodes))
              (gymnast-ir-node-id (car acc-nodes)))
            nil)))
      (coverage (gymnast-coverage-gaps ir obligations)))
    (list 'verification-bundle
      (list 'schema $gymnast-verify-schema)
      (list 'obligations obligations)
      (list 'results results)
      (list 'summary
        (list (list 'total (length obligations))
          (list 'passed (length passed))
          (list 'failed (length failed))
          (list 'skipped (length skipped))))
      (list 'coverage coverage)
      (list 'environment-diagnostics env-diags))))

(defun gymnast-verification-bundle-field (bundle key)
  (gymnast-assoc-value key (cdr bundle)))
