;;; Executable transition calculus.
;;;
;;; Behavior clauses define a reference transition system over the
;;; semantic state model.  This module extracts formal transitions,
;;; type-checks their references against the design graph, and
;;; provides a bounded reference interpreter that produces stable
;;; counterexamples for illegal state transitions.

;;; Transition model extraction from behavior IR nodes.

(defun gymnast-clause-head (clause) (car clause))
(defun gymnast-clause-body (clause) (cdr clause))

(defun gymnast-collect-clauses (clauses head)
  (filter (lambda (c) (equal (gymnast-clause-head c) head)) clauses))

(defun gymnast-parse-on-spec (on-spec)
  (cond
    ((null on-spec) (list nil nil nil))
    ((atom on-spec) (list (gymnast-symbol-string on-spec) nil nil))
    ((and (> (length on-spec) 2)
        (equal (cadr on-spec) '/))
      (list
        (concat (gymnast-symbol-string (car on-spec))
          "/" (gymnast-symbol-string (caddr on-spec)))
        (if (> (length on-spec) 3) (cadddr on-spec) nil)
        (if (> (length on-spec) 4)
          (car (cdr (cdr (cdr (cdr on-spec))))) nil)))
    (t
      (list
        (gymnast-symbol-string (car on-spec))
        (if (cdr on-spec) (cadr on-spec) nil)
        (if (cdr (cdr on-spec)) (caddr on-spec) nil)))))

(defun gymnast-extract-transition (ir-node)
  (let* ((id (gymnast-ir-node-id ir-node))
      (fields (gymnast-ir-node-field ir-node 'fields))
      (clauses (gymnast-ir-node-field ir-node 'clauses))
      (on-spec (gymnast-assoc-value ':on fields))
      (parsed-on (gymnast-parse-on-spec on-spec))
      (reads (gymnast-assoc-value ':reads fields))
      (writes (gymnast-assoc-value ':writes fields))
      (atomic (gymnast-assoc-value ':atomic fields))
      (idempotency (gymnast-assoc-value ':idempotency fields))
      (requires (gymnast-collect-clauses clauses 'requires))
      (ensures (gymnast-collect-clauses clauses 'ensures))
      (returns (gymnast-collect-clauses clauses 'returns))
      (fails (gymnast-collect-clauses clauses 'fails))
      (emits (gymnast-collect-clauses clauses 'emits)))
    (list 'transition
      (list 'id id)
      (list 'operation (car parsed-on))
      (list 'actor (cadr parsed-on))
      (list 'input (caddr parsed-on))
      (list 'reads (if (consp reads) reads (if reads (list reads) nil)))
      (list 'writes (if (consp writes) writes (if writes (list writes) nil)))
      (list 'atomic atomic)
      (list 'idempotency idempotency)
      (list 'preconditions (mapcar #'gymnast-clause-body requires))
      (list 'postconditions (mapcar #'gymnast-clause-body ensures))
      (list 'result (if returns (gymnast-clause-body (car returns)) nil))
      (list 'failures (mapcar #'gymnast-clause-body fails))
      (list 'emissions (mapcar #'gymnast-clause-body emits)))))

(defun gymnast-transition-field (tr key)
  (gymnast-assoc-value key (cdr tr)))

(defun gymnast-extract-transitions (ir)
  (mapcar #'gymnast-extract-transition
    (gymnast-ir-nodes-of-kind ir 'behavior)))

;;; Reference checking: validate that transition references resolve
;;; against the design and obligation graphs.

(defun gymnast-check-state-refs (ir refs subject)
  (if (null refs)
    nil
    (let ((ref (car refs)))
      (append
        (if (gymnast-any
            (lambda (node)
              (equal (gymnast-ir-node-field node 'name) ref))
            (gymnast-ir-nodes-of-kind ir 'state))
          nil
          (list (gymnast-diagnostic 'warning 'unresolved-state-ref subject
              (concat "state reference not found: "
                (princ-to-string ref))
              ref)))
        (gymnast-check-state-refs ir (cdr refs) subject)))))

(defun gymnast-check-transition-refs (ir transition)
  (let* ((id (gymnast-transition-field transition 'id))
      (reads (gymnast-transition-field transition 'reads))
      (writes (gymnast-transition-field transition 'writes)))
    (append
      (gymnast-check-state-refs ir reads id)
      (gymnast-check-state-refs ir writes id))))

(defun gymnast-check-all-transitions (ir transitions)
  (reduce #'append
    (mapcar (lambda (tr) (gymnast-check-transition-refs ir tr))
      transitions)
    nil))

;;; Reference state machine.

(defun gymnast-make-initial-state (ir)
  (mapcar
    (lambda (node)
      (let ((name (gymnast-ir-node-field node 'name))
          (initial (gymnast-surface-field node ':initial)))
        (list name (if (equal initial 'empty) nil initial))))
    (gymnast-ir-nodes-of-kind ir 'state)))

(defun gymnast-state-read (state ref)
  (gymnast-assoc-value ref state))

(defun gymnast-state-write (state ref value)
  (gymnast-put-assoc ref value state))

;;; Predicate evaluation (symbolic with concrete fallbacks).

(defun gymnast-eval-predicate (pred state actor input)
  (cond
    ((null pred) t)
    ((atom pred) t)
    ((equal (car pred) '=)
      (equal (gymnast-eval-expr (cadr pred) state actor input)
        (gymnast-eval-expr (caddr pred) state actor input)))
    ((equal (car pred) 'not)
      (not (gymnast-eval-predicate (cadr pred) state actor input)))
    ((equal (car pred) 'and)
      (gymnast-all
        (lambda (p) (gymnast-eval-predicate p state actor input))
        (cdr pred)))
    ((equal (car pred) 'or)
      (gymnast-any
        (lambda (p) (gymnast-eval-predicate p state actor input))
        (cdr pred)))
    ((equal (car pred) '<)
      (< (gymnast-eval-expr (cadr pred) state actor input)
        (gymnast-eval-expr (caddr pred) state actor input)))
    ((equal (car pred) '<=)
      (<= (gymnast-eval-expr (cadr pred) state actor input)
        (gymnast-eval-expr (caddr pred) state actor input)))
    (t t)))

(defun gymnast-eval-expr (expr state actor input)
  (cond
    ((null expr) nil)
    ((numberp expr) expr)
    ((stringp expr) expr)
    ((equal expr 'pre) state)
    ((equal expr 'post) state)
    ((equal expr 'actor) actor)
    ((equal expr 'input) input)
    ((equal expr 'result) 'result-placeholder)
    ((atom expr) (or (gymnast-assoc-value expr state) expr))
    (t expr)))

;;; Trace execution.

(defun gymnast-make-trace-step (transition pre-state post-state
    actor input result outcome)
  (list 'trace-step
    (list 'transition-id (gymnast-transition-field transition 'id))
    (list 'actor actor)
    (list 'input input)
    (list 'pre-state pre-state)
    (list 'post-state post-state)
    (list 'result result)
    (list 'outcome outcome)))

(defun gymnast-check-preconditions (transition state actor input)
  (let ((preds (gymnast-transition-field transition 'preconditions)))
    (gymnast-all
      (lambda (pred) (gymnast-eval-predicate (car pred) state actor input))
      preds)))

(defun gymnast-find-matching-failure (transition state actor input)
  (let* ((failures (gymnast-transition-field transition 'failures))
      (matching
        (filter
          (lambda (failure)
            (let ((when-pred (gymnast-keyword-value failure ':when)))
              (if when-pred
                (gymnast-eval-predicate when-pred state actor input)
                nil)))
          failures)))
    (if matching (car matching) nil)))

(defun gymnast-apply-transition (transition state actor input)
  (let ((failure (gymnast-find-matching-failure
          transition state actor input)))
    (if failure
      (let ((error-name (car failure))
          (preserves (gymnast-keyword-value failure ':preserves)))
        (gymnast-make-trace-step transition state
          (if (equal preserves 'all-state) state state)
          actor input nil
          (list 'failed error-name)))
      (if (gymnast-check-preconditions transition state actor input)
        (let* ((writes (gymnast-transition-field transition 'writes))
            (post-state (reduce
                (lambda (s ref)
                  (gymnast-state-write s ref
                    (append (or (gymnast-state-read s ref) nil)
                      (list input))))
                writes state)))
          (gymnast-make-trace-step transition state post-state
            actor input input (list 'succeeded)))
        (gymnast-make-trace-step transition state state
          actor input nil (list 'precondition-failed))))))

;;; Invariant checking against state.

(defun gymnast-check-invariants (ir state)
  (let ((invariants (gymnast-ir-nodes-of-kind ir 'invariant)))
    (filter (lambda (x) x)
      (mapcar
        (lambda (inv)
          (let* ((always (gymnast-surface-field inv ':always))
              (holds (gymnast-eval-predicate always state nil nil)))
            (if holds nil
              (list 'violation
                (list 'invariant (gymnast-ir-node-id inv))
                (list 'predicate always)
                (list 'state state)))))
        invariants))))

;;; Bounded trace execution.

(defun gymnast-execute-trace-steps (ir transitions steps state
    results violations bound)
  (if (or (null steps) (<= bound 0))
    (list 'trace
      (list 'steps (reverse results))
      (list 'violations violations)
      (list 'final-state state))
    (let* ((step (car steps))
        (op-name (car step))
        (actor (cadr step))
        (input (caddr step))
        (matching (filter
            (lambda (tr)
              (equal (gymnast-transition-field tr 'operation) op-name))
            transitions))
        (transition (if matching (car matching) nil)))
      (if (not transition)
        (let ((error-result
              (list 'trace-step
                (list 'transition-id 'unknown)
                (list 'actor actor)
                (list 'input input)
                (list 'pre-state state)
                (list 'post-state state)
                (list 'result nil)
                (list 'outcome (list 'no-matching-transition op-name)))))
          (gymnast-execute-trace-steps ir transitions (cdr steps)
            state (cons error-result results)
            (cons (list 'violation
                (list 'type 'no-matching-transition)
                (list 'operation op-name))
              violations)
            (- bound 1)))
        (let* ((result (gymnast-apply-transition
                transition state actor input))
            (post (gymnast-assoc-value 'post-state (cdr result)))
            (inv-violations (gymnast-check-invariants ir post)))
          (gymnast-execute-trace-steps ir transitions (cdr steps)
            post (cons result results)
            (append violations inv-violations)
            (- bound 1)))))))

(defun gymnast-execute-trace (ir steps)
  (let* ((transitions (gymnast-extract-transitions ir))
      (state (gymnast-make-initial-state ir)))
    (gymnast-execute-trace-steps ir transitions steps state
      nil nil 1000)))

(defun gymnast-trace-field (trace key)
  (gymnast-assoc-value key (cdr trace)))

(defun gymnast-trace-violations (trace)
  (gymnast-trace-field trace 'violations))

;;; Counterexample production.

(defun gymnast-counterexample (violation trace-step)
  (list 'counterexample
    (list 'violation violation)
    (list 'trace-step trace-step)
    (list 'pre-state
      (gymnast-assoc-value 'pre-state (cdr trace-step)))
    (list 'input
      (gymnast-assoc-value 'input (cdr trace-step)))
    (list 'outcome
      (gymnast-assoc-value 'outcome (cdr trace-step)))))
