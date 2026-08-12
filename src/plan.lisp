;;; Deterministic lowering from semantic IR to a typed synthesis DAG.

(defun gymnast-surface-field (node key)
  (gymnast-assoc-value key (gymnast-ir-node-field node 'fields)))

(defun gymnast-first-synthesis-node (ir)
  (let ((nodes (gymnast-ir-nodes-of-kind ir 'synthesis)))
    (if nodes (car nodes) nil)))

(defun gymnast-selected-target (ir)
  (let ((node (gymnast-first-synthesis-node ir)))
    (if node
        (or (gymnast-surface-field node ':target) '(lamedh :track "0.5"))
        '(lamedh :track "0.5"))))

(defun gymnast-selected-model (ir)
  (let ((node (gymnast-first-synthesis-node ir)))
    (if node
        (or (gymnast-surface-field node ':model)
            '(small-code-model :class nano))
        '(small-code-model :class nano))))

(defun gymnast-plan-id (ir local-name)
  (concat (gymnast-symbol-string
            (gymnast-assoc-value 'name (gymnast-ir-field ir 'module)))
          "/plan/" local-name))

(defun gymnast-ids-for-kinds (ir kinds)
  (gymnast-ir-node-ids
    (filter (lambda (node) (member (gymnast-ir-node-kind node) kinds))
            (gymnast-ir-all-nodes ir))))

(defun gymnast-build-plan-nodes (ir)
  (let* ((target (gymnast-selected-target ir))
         (model (gymnast-selected-model ir))
         (design-id (gymnast-plan-id ir "design-contracts"))
         (transition-id (gymnast-plan-id ir "transition-kernel"))
         (auth-id (gymnast-plan-id ir "authorization-policy"))
         (persistence-id (gymnast-plan-id ir "persistence"))
         (interface-id (gymnast-plan-id ir "interface-contracts"))
         (handler-id (gymnast-plan-id ir "service-handlers"))
         (acceptance-id (gymnast-plan-id ir "acceptance-harness"))
         (assembly-id (gymnast-plan-id ir "application-assembly"))
         (design
           (gymnast-plan-node
             design-id 'structural 'design-contracts/v1
             (gymnast-ids-for-kinds ir '(actor type component flow))
             nil target '(none)
             '("generated/design/contracts.lisp") nil
             '(well-formed-types explicit-capability-edges)
             '(invent-product-semantics add-dependencies)))
         (transitions
           (gymnast-plan-node
             transition-id 'generative 'transition-kernel/v1
             (gymnast-ids-for-kinds ir '(type state behavior invariant))
             (list design-id) target model
             '("generated/domain/transitions.lisp")
             '(clock id-source)
             '(implements-transition-system preserves-invariants
               deterministic-under-same-input)
             '(perform-io weaken-preconditions invent-errors)))
         (authorization
           (gymnast-plan-node
             auth-id 'generative 'authorization-policy/v1
             (gymnast-ids-for-kinds ir '(actor flow behavior invariant))
             (list design-id transition-id) target model
             '("generated/domain/authorization.lisp") nil
             '(deny-by-default noninterference owner-isolation)
             '(grant-undeclared-capabilities reveal-resource-existence)))
         (persistence
           (gymnast-plan-node
             persistence-id 'generative 'persistence/v1
             (gymnast-ids-for-kinds ir '(type state behavior constraint))
             (list design-id transition-id) target model
             '("generated/adapters/persistence.lisp"
               "generated/adapters/schema.sexpr")
             '(durable-store transactions)
             '(durable-commit atomic-boundaries retry-safety)
             '(perform-network-io choose-unpinned-dependencies)))
         (interfaces
           (gymnast-plan-node
             interface-id 'structural 'interface-contracts/v1
             (gymnast-ids-for-kinds ir '(type interface))
             (list design-id) target '(none)
             '("generated/interfaces/contracts.lisp") nil
             '(complete-operation-surface declared-errors-only)
             '(change-observable-contract)))
         (handlers
           (gymnast-plan-node
             handler-id 'generative 'service-handlers/v1
             (gymnast-ids-for-kinds ir '(interface behavior state constraint))
             (list transition-id auth-id persistence-id interface-id)
             target model
             '("generated/service/handlers.lisp")
             '(repository identity clock id-source)
             '(contract-conformance authorization-before-observation
               idempotent-retries)
             '(access-filesystem access-network add-endpoints)))
         (acceptance
           (gymnast-plan-node
             acceptance-id 'verification 'acceptance-harness/v1
             (gymnast-ids-for-kinds
               ir '(behavior invariant constraint acceptance interface state))
             (list handler-id) target '(none)
             '("generated/verification/acceptance.lisp") nil
             '(independent-oracle trace-equivalence boundary-coverage
               deterministic-execution)
             '(read-generated-rationale weaken-obligations skip-failures)))
         (assembly
           (gymnast-plan-node
             assembly-id 'assembly 'application-assembly/v1
             (gymnast-ids-for-kinds
               ir '(application import component synthesis constraint))
             (list transition-id auth-id persistence-id interface-id
                   handler-id acceptance-id)
             target '(none)
             '("generated/application.lisp" "generated/manifest.sexpr") nil
             '(all-artifacts-linked all-obligations-addressed)
             '(untracked-artifacts undeclared-capabilities))))
    (list design transitions authorization persistence interfaces handlers
          acceptance assembly)))

(defun gymnast-plan-dependency-diagnostics (nodes)
  (let ((ids (mapcar #'gymnast-plan-node-id nodes)))
    (reduce
      #'append
      (mapcar
        (lambda (node)
          (mapcar
            (lambda (missing)
              (gymnast-diagnostic
                'error 'missing-plan-dependency
                (gymnast-plan-node-id node)
                "plan node depends on an unknown node" missing))
            (filter
              (lambda (dependency) (not (member dependency ids)))
              (gymnast-plan-node-field node 'depends-on))))
        nodes)
      nil)))

(defun gymnast-coverage-entries (ir nodes)
  (mapcar
    (lambda (ir-node)
      (let ((id (gymnast-ir-node-id ir-node)))
        (list id
              (mapcar
                #'gymnast-plan-node-id
                (filter
                  (lambda (plan-node)
                    (member id (gymnast-plan-node-field plan-node 'inputs)))
                  nodes)))))
    (gymnast-ir-all-nodes ir)))

(defun gymnast-coverage-diagnostics (coverage)
  (mapcar
    (lambda (entry)
      (gymnast-diagnostic
        'error 'unplanned-semantic-node (car entry)
        "semantic node has no implementation or evidence path" entry))
    (filter (lambda (entry) (null (cadr entry))) coverage)))

(defun gymnast-plan (ir)
  (gymnast-assert-valid-ir ir)
  (let* ((nodes (gymnast-build-plan-nodes ir))
         (coverage (gymnast-coverage-entries ir nodes))
         (diagnostics
           (append (gymnast-plan-dependency-diagnostics nodes)
                   (gymnast-coverage-diagnostics coverage)))
         (base
           (list 'plan
                 (list 'schema $gymnast-plan-schema)
                 (list 'ir-fingerprint (gymnast-ir-field ir 'fingerprint))
                 (list 'target (gymnast-selected-target ir))
                 (list 'nodes nodes)
                 (list 'coverage coverage)
                 (list 'diagnostics diagnostics))))
    (append base (list (list 'fingerprint (gymnast-fingerprint base))))))

(defun gymnast-assert-valid-plan (plan)
  (let ((diagnostics (gymnast-plan-field plan 'diagnostics)))
    (if (gymnast-has-errors-p diagnostics)
        (error (concat "planning failed: " (prin1-to-string diagnostics)))
        plan)))

