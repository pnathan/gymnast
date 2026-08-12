;;; Assembly and promotion evidence bundles.
;;;
;;; Local node success must culminate in one reproducible executable
;;; artifact with complete traceability.  This module links declared
;;; artifacts, validates capability edges, builds the traceability
;;; map, and packages evidence for promotion review.
;;;
;;; Promotion policy is defined separately from generation: the
;;; assembler only collects evidence; it never decides whether the
;;; evidence is sufficient.

(def $gymnast-bundle-schema "gymnast.bundle/0.1")

(defrecord gymnast-artifact path node-id digest size)

(defrecord gymnast-traceability-entry semantic-id kind plan-nodes
  has-implementation has-evidence)

;;; Artifact linking: collect all candidate file outputs and validate
;;; that every declared artifact is present and no untracked files exist.

(defun gymnast-collect-artifacts (results)
  (reduce #'append
    (mapcar
      (lambda (result)
        (let ((candidate (gymnast-assoc-value 'candidate (cdr result))))
          (if (and candidate (gymnast-tagged-p 'candidate candidate))
            (let ((files (gymnast-candidate-field candidate 'files)))
              (mapcar (lambda (f)
                  (make-gymnast-artifact (car f)
                    (gymnast-assoc-value 'node-id (cdr result))
                    (gymnast-fingerprint-string (cadr f))
                    (length (cadr f))))
                (or files nil)))
            nil)))
      results)
    nil))

(defun gymnast-artifact-field (artifact key)
  (record-ref artifact key))

;;; Validate that artifacts match declared may-write paths.

(defun gymnast-validate-artifacts (plan artifacts)
  (let* ((nodes (gymnast-plan-field plan 'nodes))
      (declared-paths (reduce #'append
          (mapcar (lambda (n)
              (gymnast-plan-node-field n 'may-write))
            nodes)
          nil))
      (actual-paths (mapcar
          (lambda (a) (gymnast-artifact-field a 'path))
          artifacts))
      (untracked (filter
          (lambda (path) (not (member path declared-paths)))
          actual-paths))
      (missing (filter
          (lambda (path) (not (member path actual-paths)))
          declared-paths)))
    (append
      (mapcar (lambda (path)
          (gymnast-diagnostic 'error 'untracked-artifact
            path "artifact not declared in any plan node" path))
        untracked)
      (mapcar (lambda (path)
          (gymnast-diagnostic 'warning 'missing-artifact
            path "declared artifact not produced" path))
        missing))))

;;; Capability edge validation.

(defun gymnast-validate-capability-edges (plan)
  (let* ((nodes (gymnast-plan-field plan 'nodes))
      (all-capabilities
        (reduce #'append
          (mapcar (lambda (n)
              (gymnast-plan-node-field n 'capabilities))
            nodes)
          nil))
      (all-prohibitions
        (reduce #'append
          (mapcar (lambda (n)
              (gymnast-plan-node-field n 'prohibitions))
            nodes)
          nil))
      (violations (filter
          (lambda (cap) (member cap all-prohibitions))
          all-capabilities)))
    (mapcar (lambda (cap)
        (gymnast-diagnostic 'error 'prohibited-capability
          (princ-to-string cap)
          "capability is both used and prohibited"
          cap))
      violations)))

;;; Traceability map: source -> IR -> plan node -> evidence.

(defun gymnast-traceability-entry (ir-node plan results)
  (let* ((id (gymnast-ir-node-id ir-node))
      (plan-nodes (filter
          (lambda (n)
            (member id (gymnast-plan-node-field n 'inputs)))
          (gymnast-plan-field plan 'nodes)))
      (plan-ids (mapcar #'gymnast-plan-node-id plan-nodes))
      (evidence (filter
          (lambda (r)
            (member (gymnast-assoc-value 'node-id (cdr r)) plan-ids))
          results)))
    (make-gymnast-traceability-entry id
      (gymnast-ir-node-kind ir-node)
      plan-ids
      (> (length plan-ids) 0)
      (> (length evidence) 0))))

(defun gymnast-traceability-entry-field (entry key)
  (record-ref entry key))

(defun gymnast-build-traceability-map (ir plan results)
  (mapcar
    (lambda (node)
      (gymnast-traceability-entry node plan results))
    (gymnast-ir-all-nodes ir)))

(defun gymnast-traceability-diagnostics (traceability)
  (reduce #'append
    (mapcar
      (lambda (entry)
        (let ((id (gymnast-traceability-entry-field entry 'semantic-id))
            (has-impl (gymnast-traceability-entry-field
                entry 'has-implementation)))
          (if has-impl nil
            (list (gymnast-diagnostic 'warning 'unimplemented-semantic-node
                id "semantic node has no implementation path" id)))))
      traceability)
    nil))

;;; Dependency lock: snapshot of exact recipe and tool versions.

(defun gymnast-dependency-lock (plan)
  (let ((nodes (gymnast-plan-field plan 'nodes)))
    (list 'dependency-lock
      (list 'plan-fingerprint (gymnast-plan-field plan 'fingerprint))
      (list 'node-locks
        (mapcar
          (lambda (node)
            (list 'node-lock
              (list 'node-id (gymnast-plan-node-id node))
              (list 'recipe (gymnast-plan-node-field node 'recipe))
              (list 'model (gymnast-plan-node-field node 'model))
              (list 'fingerprint
                (gymnast-plan-node-field node 'fingerprint))))
          nodes)))))

(defun gymnast-dependency-lock-field (lock key)
  (gymnast-assoc-value key (cdr lock)))

;;; Promotion policy.
;;;
;;; The promotion policy defines what evidence is required before
;;; an assembly can be promoted.  It is separate from generation:
;;; the assembler collects evidence; the policy evaluates it.

(defun gymnast-default-promotion-policy ()
  (list 'promotion-policy
    (list 'name 'default)
    (list 'requires
      (list
        (list 'all-artifacts-present t)
        (list 'no-untracked-artifacts t)
        (list 'no-capability-violations t)
        (list 'all-nodes-succeeded t)
        (list 'verification-passed t)
        (list 'traceability-complete t)))))

(defun gymnast-promotion-policy-field (policy key)
  (gymnast-assoc-value key (cdr policy)))

(defun gymnast-evaluate-promotion (policy bundle)
  (let* ((requirements (gymnast-promotion-policy-field policy 'requires))
      (diagnostics (gymnast-bundle-field bundle 'diagnostics))
      (has-errors (gymnast-has-errors-p diagnostics))
      (summary (gymnast-bundle-field bundle 'summary))
      (all-succeeded (equal
          (gymnast-assoc-value 'failed-nodes summary) 0))
      (traceability (gymnast-bundle-field bundle 'traceability))
      (all-traced (gymnast-all
          (lambda (entry)
            (gymnast-traceability-entry-field entry 'has-implementation))
          traceability))
      (checks
        (list
          (list 'no-error-diagnostics (not has-errors))
          (list 'all-nodes-succeeded all-succeeded)
          (list 'traceability-complete all-traced)))
      (all-pass (gymnast-all #'cadr checks)))
    (list 'promotion-result
      (list 'policy (gymnast-promotion-policy-field policy 'name))
      (list 'decision (if all-pass 'promote 'hold))
      (list 'checks checks))))

(defun gymnast-promotion-result-field (result key)
  (gymnast-assoc-value key (cdr result)))

;;; Evidence bundle assembly.

(defun gymnast-assemble-bundle (ir plan execution-results
    verification-bundle)
  (let* ((artifacts (gymnast-collect-artifacts execution-results))
      (artifact-diags (gymnast-validate-artifacts plan artifacts))
      (cap-diags (gymnast-validate-capability-edges plan))
      (traceability
        (gymnast-build-traceability-map ir plan execution-results))
      (trace-diags (gymnast-traceability-diagnostics traceability))
      (lock (gymnast-dependency-lock plan))
      (succeeded (filter
          (lambda (r)
            (let ((status (gymnast-assoc-value 'status (cdr r))))
              (or (equal status 'succeeded) (equal status 'passed))))
          execution-results))
      (failed (filter
          (lambda (r)
            (let ((status (gymnast-assoc-value 'status (cdr r))))
              (equal status 'failed)))
          execution-results))
      (all-diags (append artifact-diags cap-diags trace-diags))
      (summary
        (list
          (list 'total-nodes
            (length (gymnast-plan-field plan 'nodes)))
          (list 'artifacts-produced (length artifacts))
          (list 'succeeded-nodes (length succeeded))
          (list 'failed-nodes (length failed))
          (list 'has-verification
            (not (null verification-bundle))))))
    (list 'evidence-bundle
      (list 'schema $gymnast-bundle-schema)
      (list 'ir-fingerprint (gymnast-ir-field ir 'fingerprint))
      (list 'plan-fingerprint (gymnast-plan-field plan 'fingerprint))
      (list 'artifacts artifacts)
      (list 'traceability traceability)
      (list 'dependency-lock lock)
      (list 'verification verification-bundle)
      (list 'summary summary)
      (list 'diagnostics all-diags))))

(defun gymnast-bundle-field (bundle key)
  (gymnast-assoc-value key (cdr bundle)))
