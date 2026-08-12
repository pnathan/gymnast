;;; Platform kit registry.
;;;
;;; A platform kit is a versioned collection of capability adapters
;;; that form the trusted runtime boundary for synthesized applications.
;;; Generated code must cross capability interfaces for all external
;;; effects; direct stdlib access is a synthesis prohibition.

(defun gymnast-platform-prop-key (version)
  (concat "gymnast.platform/"
    (if (stringp version) version (princ-to-string version))))

(defun gymnast-define-capability (name version guarantees failure-modes)
  (list 'capability
    (list 'name name)
    (list 'version version)
    (list 'guarantees guarantees)
    (list 'failure-modes failure-modes)))

(defun gymnast-capability-field (cap key)
  (gymnast-assoc-value key (cdr cap)))

(defun gymnast-define-platform-kit (name version target capabilities)
  (let ((kit
        (list 'platform-kit
          (list 'name name)
          (list 'version version)
          (list 'target target)
          (list 'capabilities capabilities))))
    (putp name (gymnast-platform-prop-key version) kit)
    kit))

(defun gymnast-lookup-platform-kit (name version)
  (if version
    (getp name (gymnast-platform-prop-key version))
    nil))

(defun gymnast-platform-kit-field (kit key)
  (gymnast-assoc-value key (cdr kit)))

(defun gymnast-platform-kit-capability-names (kit)
  (mapcar
    (lambda (cap) (gymnast-capability-field cap 'name))
    (gymnast-platform-kit-field kit 'capabilities)))

(defun gymnast-validate-node-capabilities (node kit)
  (let ((declared (gymnast-plan-node-field node 'capabilities))
      (available (gymnast-platform-kit-capability-names kit))
      (node-id (gymnast-plan-node-id node)))
    (filter (lambda (x) x)
      (mapcar
        (lambda (cap)
          (if (or (equal cap '(none)) (member cap available))
            nil
            (gymnast-diagnostic 'error 'undeclared-capability node-id
              (concat "capability not provided by platform kit: "
                (princ-to-string cap))
              cap)))
        declared))))

(defun gymnast-validate-plan-capabilities (plan kit)
  (let ((nodes (gymnast-plan-field plan 'nodes)))
    (reduce #'append
      (mapcar (lambda (node) (gymnast-validate-node-capabilities node kit))
        nodes)
      nil)))

;;; Reference platform kit: gymnast-ruby-platform-v1
;;;
;;; Capabilities: identity, persistence, transactions, clock, id-source,
;;; http, telemetry, lifecycle.  Each has characterized guarantees and
;;; declared failure modes that generated code must handle.

(def $gymnast-ruby-platform-capabilities
  (list
    (gymnast-define-capability 'identity "1.0"
      '(token-validation session-binding principal-extraction)
      '(unauthenticated token-expired provider-unavailable))
    (gymnast-define-capability 'persistence "1.0"
      '(durable-commit read-after-write)
      '(connection-lost constraint-violation not-found))
    (gymnast-define-capability 'repository "1.0"
      '(typed-queries aggregate-loading optimistic-locking)
      '(not-found version-conflict connection-lost))
    (gymnast-define-capability 'transactions "1.0"
      '(atomic-boundaries rollback-on-error serializable-per-scope)
      '(deadlock timeout rollback))
    (gymnast-define-capability 'clock "1.0"
      '(monotonic utc-wall-time virtual-in-tests)
      '(drift-beyond-tolerance))
    (gymnast-define-capability 'id-source "1.0"
      '(globally-unique collision-resistant sortable)
      '(entropy-exhausted))
    (gymnast-define-capability 'http "1.0"
      '(request-routing content-negotiation error-mapping)
      '(bad-request method-not-allowed internal-error))
    (gymnast-define-capability 'telemetry "1.0"
      '(structured-logging request-tracing metric-emission)
      '(buffer-overflow))
    (gymnast-define-capability 'lifecycle "1.0"
      '(graceful-shutdown health-check dependency-ordering)
      '(startup-failure shutdown-timeout))
    (gymnast-define-capability 'durable-store "1.0"
      '(durable-commit read-after-write schema-migration)
      '(connection-lost constraint-violation))))

(def $gymnast-ruby-platform-kit
  (gymnast-define-platform-kit
    'gymnast-ruby-platform-v1 "1.0" 'ruby
    $gymnast-ruby-platform-capabilities))
