;;; Adequacy campaign with mutation, concurrency, and fault injection.
;;;
;;; Passing happy-path tests is insufficient evidence that the
;;; verifier can detect realistic synthesis defects.  This module
;;; seeds known defects, runs verification against the mutated
;;; system, and reports which obligations detected each defect.
;;;
;;; A campaign fails when important mutants survive undetected.

(def $gymnast-adequacy-schema "gymnast.adequacy/0.1")

(defrecord gymnast-mutant id class description mutator critical)

(defrecord gymnast-mutant-result mutant-id class critical killed
  detecting-obligations description)

(defrecord gymnast-fault-scenario name type after expected)

;;; Mutation operators.
;;;
;;; Each mutant modifies a specific aspect of the transition system
;;; to simulate a realistic synthesis defect.  Mutants are applied
;;; to the IR, which is then re-verified.

(defun gymnast-mutant (id class description mutator)
  (make-gymnast-mutant id class description mutator t))

(defun gymnast-mutant-field (mutant key)
  (record-ref mutant key))

;;; Built-in mutation operators for the Todo specification.

(defun gymnast-mutate-weaken-precondition (ir behavior-name)
  (let* ((behaviors (gymnast-ir-nodes-of-kind ir 'behavior))
      (target (car (filter
            (lambda (b)
              (equal (gymnast-ir-node-field b 'name) behavior-name))
            behaviors))))
    (if (not target)
      ir
      (let* ((clauses (gymnast-ir-node-field target 'clauses))
          (weakened (filter
              (lambda (c)
                (not (equal (gymnast-clause-head c) 'requires)))
              clauses))
          (mutated-node (gymnast-ir-node
              (gymnast-ir-node-id target)
              (gymnast-ir-node-kind target)
              (gymnast-ir-node-field target 'name)
              (gymnast-ir-node-field target 'fields)
              weakened
              (gymnast-ir-node-field target 'surface-mechanism))))
        (gymnast-replace-ir-node ir target mutated-node)))))

(defun gymnast-mutate-remove-invariant (ir invariant-name)
  (let* ((invariants (gymnast-ir-nodes-of-kind ir 'invariant))
      (target (car (filter
            (lambda (inv)
              (equal (gymnast-ir-node-field inv 'name) invariant-name))
            invariants))))
    (if (not target)
      ir
      (gymnast-remove-ir-node ir target))))

(defun gymnast-mutate-weaken-limit (ir invariant-name new-limit)
  (let* ((invariants (gymnast-ir-nodes-of-kind ir 'invariant))
      (target (car (filter
            (lambda (inv)
              (equal (gymnast-ir-node-field inv 'name) invariant-name))
            invariants))))
    (if (not target)
      ir
      (let* ((fields (gymnast-ir-node-field target 'fields))
          (old-always (gymnast-assoc-value ':always fields))
          (new-always (gymnast-replace-limit old-always new-limit))
          (new-fields (gymnast-put-assoc ':always new-always fields))
          (mutated-node (gymnast-ir-node
              (gymnast-ir-node-id target)
              'invariant
              (gymnast-ir-node-field target 'name)
              new-fields
              (gymnast-ir-node-field target 'clauses)
              (gymnast-ir-node-field target 'surface-mechanism))))
        (gymnast-replace-ir-node ir target mutated-node)))))

(defun gymnast-mutate-remove-failure-mode (ir behavior-name)
  (let* ((behaviors (gymnast-ir-nodes-of-kind ir 'behavior))
      (target (car (filter
            (lambda (b)
              (equal (gymnast-ir-node-field b 'name) behavior-name))
            behaviors))))
    (if (not target)
      ir
      (let* ((clauses (gymnast-ir-node-field target 'clauses))
          (without-fails (filter
              (lambda (c)
                (not (equal (gymnast-clause-head c) 'fails)))
              clauses))
          (mutated-node (gymnast-ir-node
              (gymnast-ir-node-id target)
              (gymnast-ir-node-kind target)
              (gymnast-ir-node-field target 'name)
              (gymnast-ir-node-field target 'fields)
              without-fails
              (gymnast-ir-node-field target 'surface-mechanism))))
        (gymnast-replace-ir-node ir target mutated-node)))))

(defun gymnast-mutate-skip-state-write (ir behavior-name)
  (let* ((behaviors (gymnast-ir-nodes-of-kind ir 'behavior))
      (target (car (filter
            (lambda (b)
              (equal (gymnast-ir-node-field b 'name) behavior-name))
            behaviors))))
    (if (not target)
      ir
      (let* ((fields (gymnast-ir-node-field target 'fields))
          (new-fields (gymnast-put-assoc ':writes nil fields))
          (mutated-node (gymnast-ir-node
              (gymnast-ir-node-id target)
              (gymnast-ir-node-kind target)
              (gymnast-ir-node-field target 'name)
              new-fields
              (gymnast-ir-node-field target 'clauses)
              (gymnast-ir-node-field target 'surface-mechanism))))
        (gymnast-replace-ir-node ir target mutated-node)))))

;;; IR mutation helpers.

(defun gymnast-replace-ir-node (ir old-node new-node)
  (let ((old-id (gymnast-ir-node-id old-node)))
    (gymnast-map-ir-sections ir
      (lambda (nodes)
        (mapcar (lambda (n)
            (if (equal (gymnast-ir-node-id n) old-id) new-node n))
          nodes)))))

(defun gymnast-remove-ir-node (ir target-node)
  (let ((target-id (gymnast-ir-node-id target-node)))
    (gymnast-map-ir-sections ir
      (lambda (nodes)
        (filter (lambda (n)
            (not (equal (gymnast-ir-node-id n) target-id)))
          nodes)))))

(defun gymnast-map-ir-sections (ir transform-fn)
  (let* ((sections '(design transitions obligations synthesis))
      (entries (cdr ir)))
    (cons 'ir
      (mapcar
        (lambda (entry)
          (if (member (car entry) sections)
            (list (car entry)
              (funcall transform-fn (cadr entry)))
            entry))
        entries))))

(defun gymnast-replace-limit (predicate new-limit)
  (cond
    ((null predicate) predicate)
    ((atom predicate) predicate)
    ((and (equal (car predicate) '<=)
        (numberp (caddr predicate)))
      (list '<= (cadr predicate) new-limit))
    ((and (equal (car predicate) '<)
        (numberp (caddr predicate)))
      (list '< (cadr predicate) new-limit))
    ((equal (car predicate) 'forall)
      (list 'forall (cadr predicate)
        (gymnast-replace-limit (caddr predicate) new-limit)))
    (t predicate)))

;;; Concurrency scenarios: adversarial interleavings.

(defun gymnast-boundary-interleaving (ir boundary-count)
  (let* ((transitions (gymnast-extract-transitions ir))
      (write-transitions (filter
          (lambda (tr)
            (gymnast-transition-field tr 'writes))
          transitions)))
    (if (null write-transitions)
      nil
      (let* ((tr (car write-transitions))
          (op (gymnast-transition-field tr 'operation))
          (steps (gymnast-generate-boundary-steps
              op boundary-count)))
        (list 'interleaving-scenario
          (list 'operation op)
          (list 'boundary boundary-count)
          (list 'steps steps)
          (list 'expected-violations 0))))))

(defun gymnast-generate-boundary-steps (operation count)
  (if (<= count 0)
    nil
    (cons
      (list operation
        (concat "actor-" (princ-to-string count))
        (concat "input-" (princ-to-string count)))
      (gymnast-generate-boundary-steps operation (- count 1)))))

;;; Fault injection scenarios.

(defun gymnast-make-fault-scenario (name fault-type after)
  (make-gymnast-fault-scenario name fault-type after 'detected))

(defun gymnast-standard-fault-scenarios ()
  (list
    (gymnast-make-fault-scenario 'restart-after-write
      'restart 'acknowledged-write)
    (gymnast-make-fault-scenario 'timeout-mid-operation
      'timeout 'operation-start)
    (gymnast-make-fault-scenario 'duplicate-delivery
      'duplicate-delivery 'acknowledged-write)
    (gymnast-make-fault-scenario 'stale-version
      'stale-version 'read)))

;;; Campaign execution.
;;;
;;; For each mutant, apply it to the IR, run the verifier, and
;;; check whether the mutation was detected (killed) or survived.

(defun gymnast-run-mutant (ir mutant)
  (let* ((mutator (gymnast-mutant-field mutant 'mutator))
      (mutated-ir (funcall mutator ir))
      (obligations (gymnast-lower-all-obligations mutated-ir))
      (results (gymnast-verify-all-obligations mutated-ir obligations))
      (failed (filter
          (lambda (r)
            (equal (gymnast-verification-result-field r 'status) 'failed))
          results))
      (killed (> (length failed) 0))
      (detecting (if killed
          (mapcar
            (lambda (r)
              (gymnast-verification-result-field r 'obligation-id))
            failed)
          nil)))
    (make-gymnast-mutant-result
      (gymnast-mutant-id mutant)
      (gymnast-mutant-class mutant)
      (gymnast-mutant-critical mutant)
      killed detecting
      (gymnast-mutant-description mutant))))

(defun gymnast-mutant-result-field (result key)
  (record-ref result key))

(defun gymnast-run-campaign (ir mutants)
  (let* ((results (mapcar
          (lambda (m) (gymnast-run-mutant ir m))
          mutants))
      (killed (filter
          (lambda (r)
            (gymnast-mutant-result-field r 'killed))
          results))
      (survived (filter
          (lambda (r)
            (not (gymnast-mutant-result-field r 'killed)))
          results))
      (critical-survived (filter
          (lambda (r)
            (gymnast-mutant-result-field r 'critical))
          survived))
      (pass (null critical-survived)))
    (list 'campaign-result
      (list 'schema $gymnast-adequacy-schema)
      (list 'total (length mutants))
      (list 'killed (length killed))
      (list 'survived (length survived))
      (list 'critical-survived (length critical-survived))
      (list 'pass pass)
      (list 'results results)
      (list 'blind-spots
        (mapcar
          (lambda (r)
            (list 'blind-spot
              (list 'mutant-id
                (gymnast-mutant-result-field r 'mutant-id))
              (list 'class
                (gymnast-mutant-result-field r 'class))
              (list 'description
                (gymnast-mutant-result-field r 'description))))
          critical-survived)))))

(defun gymnast-campaign-result-field (result key)
  (gymnast-assoc-value key (cdr result)))
