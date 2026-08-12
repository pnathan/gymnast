(include "../src/gymnast.lisp")

(defspec gymnast-test-spec
  :version "test"
  (use-profile test/profile "1")
  (application test-app :default-acceptance test)
  (actor user :kind person)
  (type UserId :opaque Text)
  (type Item :record ((id UserId) (title Text)))
  (component app :responsibility "Test application" :provides (api))
  (interface api
    (command add :actor user :input Item :output Item :errors (forbidden))
    (query list :actor user :output (List Item)))
  (state items :of (List Item) :owner app :durability durable)
  (flow access :from user :to api :kind command :grant (authenticated))
  (behavior add
    :on (api/add user item)
    :reads (items)
    :writes (items)
    :atomic items
    :idempotency command-key
    (requires (authenticated? user))
    (ensures (contains? post item)))
  (invariant no-duplicates :scope items :always (unique-ids? items))
  (constraint load :class workload :must (supports 10 concurrent-users))
  (synthesis test
    :target (lamedh :track "0.5")
    :model (small-code-model :class nano :temperature 0))
  (acceptance test
    :subject app
    (property round-trip
      :generate ((item valid-item))
      :execute (sequence (add item) (list))
      :must (contains? result item))))

(deftest fexpr-captures-unevaluated-type-forms
  (let ((decl (type Captured :record ((value NotABoundVariable)))))
    (assert-equal (gymnast-surface-kind decl) 'type)
    (assert-equal (gymnast-surface-mechanism decl) 'fexpr)
    (assert-equal (gymnast-surface-operands decl)
      '(:record ((value NotABoundVariable))))))

(deftest module-is-a-vau-capture-boundary
  (assert-equal (gymnast-surface-kind gymnast-test-spec) 'module)
  (assert-equal (gymnast-surface-mechanism gymnast-test-spec) 'vau)
  (assert-true (> (length (gymnast-surface-children gymnast-test-spec)) 10)))

(deftest trusted-macro-lowers-to-kernel-form
  (let ((first-child (car (gymnast-surface-children gymnast-test-spec))))
    (assert-equal (gymnast-surface-kind first-child) 'import)
    (assert-equal (gymnast-surface-mechanism first-child) 'fexpr)))

(deftest elaboration-is-closed-and-valid
  (let ((ir (gymnast-elaborate gymnast-test-spec)))
    (assert-false (gymnast-has-errors-p (gymnast-ir-field ir 'diagnostics)))
    (assert-equal (length (gymnast-ir-nodes-of-kind ir 'behavior)) 1)
    (assert-equal (length (gymnast-ir-nodes-of-kind ir 'acceptance)) 1)))

(deftest compilation-is-byte-stable-as-data
  (let ((a (gymnast-compile gymnast-test-spec))
      (b (gymnast-compile gymnast-test-spec)))
    (assert-equal a b)
    (assert-equal (prin1-to-string a) (prin1-to-string b))))

(deftest planner-produces-complete-typed-dag
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir)))
    (assert-equal (length (gymnast-plan-field plan 'nodes)) 8)
    (assert-false (gymnast-has-errors-p
        (gymnast-plan-field plan 'diagnostics)))
    (assert-true
      (gymnast-find-plan-node plan
        (gymnast-plan-id ir "acceptance-harness")))))

(deftest every-plan-node-has-a-stable-work-packet
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (prompts (gymnast-compile-prompts ir plan)))
    (assert-equal (length prompts) (length (gymnast-plan-field plan 'nodes)))
    (assert-true
      (gymnast-all
        (lambda (prompt) (gymnast-assoc-value 'fingerprint (cdr prompt)))
        prompts))))

(deftest candidate-firewall-enforces-write-set
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (gymnast-find-plan-node
          plan (gymnast-plan-id ir "interface-contracts")))
      (good
        (list 'candidate
          (list 'schema $gymnast-candidate-schema)
          (list 'node-id (gymnast-plan-node-id node))
          (list 'files
            (list (list "generated/interfaces/contracts.lisp"
                "(def api-contract 'ok)")))
          (list 'implements (gymnast-plan-node-field node 'inputs))
          (list 'edge-uses nil)
          (list 'assumptions nil)
          (list 'unresolved nil)))
      (bad
        (gymnast-put-assoc
          'files (list (list "src/compiler.lisp" "malicious")) (cdr good))))
    (assert-true (gymnast-candidate-valid-p node good))
    (assert-false (gymnast-candidate-valid-p node (cons 'candidate bad)))))

(deftest unresolved-product-decisions-stop-elaboration
  (let* ((surface
        (module blocked
          (constraint choice :class policy :status unresolved)))
      (ir (gymnast-elaborate surface)))
    (assert-true (gymnast-has-errors-p (gymnast-ir-field ir 'diagnostics)))))

;;; Versioned semantic profile tests.

(gymnast-define-profile 'test-profile "1.0"
  '((:sharing-limit integer 100)
    (:identity-provider symbol required))
  (lambda (args)
    (let ((limit (gymnast-assoc-value ':sharing-limit args)))
      (list
        (gymnast-make-surface 'invariant 'profile-limit
          (list ':scope 'items
            ':always (list '<= 'count limit)
            ':profile-source (list 'test-profile "1.0"))
          nil 'profile)))))

(defspec profile-test-spec
  :version "test"
  (use-profile test-profile "1.0"
    :sharing-limit 50
    :identity-provider google)
  (actor user :kind person)
  (type UserId :opaque Text)
  (component app :responsibility "Profile test" :provides (api))
  (interface api
    (command add :actor user :input UserId :output UserId
      :errors (forbidden)))
  (state items :of (List UserId) :owner app :durability durable)
  (flow access :from user :to api :kind command :grant (authenticated))
  (behavior add
    :on (api/add user item)
    :reads (items) :writes (items) :atomic items
    (requires (authenticated? user))
    (ensures (contains? post item)))
  (constraint load :class workload :must (supports 10 concurrent-users))
  (synthesis test
    :target (lamedh :track "0.5")
    :model (small-code-model :class nano :temperature 0))
  (acceptance test :subject app
    (property round-trip
      :generate ((item valid-item))
      :execute (sequence (add item))
      :must (ok? result))))

(deftest profile-expands-into-kernel-declarations
  (let* ((ir (gymnast-elaborate profile-test-spec))
      (invariants (gymnast-ir-nodes-of-kind ir 'invariant)))
    (assert-true (> (length invariants) 0))
    (assert-true (gymnast-find-ir-node ir
        "profile-test-spec/invariant/profile-limit"))))

(deftest profile-provenance-tracked-in-ir
  (let* ((ir (gymnast-elaborate profile-test-spec))
      (node (gymnast-find-ir-node ir
          "profile-test-spec/invariant/profile-limit"))
      (fields (gymnast-ir-node-field node 'fields))
      (source (gymnast-assoc-value ':profile-source fields)))
    (assert-equal source (list 'test-profile "1.0"))))

(deftest profile-version-in-compilation-identity
  (let* ((a (gymnast-compile profile-test-spec))
      (b (gymnast-compile profile-test-spec)))
    (assert-equal
      (gymnast-compilation-field a 'fingerprint)
      (gymnast-compilation-field b 'fingerprint))))

(deftest profile-missing-required-decision-is-error
  (gymnast-define-profile 'strict-profile "1.0"
    '((:required-param symbol required))
    (lambda (args) nil))
  (let* ((surface
        (module missing-decision
          (use-profile strict-profile "1.0")))
      (ir (gymnast-elaborate surface)))
    (assert-true (gymnast-has-errors-p (gymnast-ir-field ir 'diagnostics)))))

(deftest profile-default-values-applied
  (gymnast-define-profile 'default-profile "1.0"
    '((:limit integer 42))
    (lambda (args)
      (let ((limit (gymnast-assoc-value ':limit args)))
        (list
          (gymnast-make-surface 'constraint 'profile-constraint
            (list ':class 'workload
              ':must (list 'supports limit 'users))
            nil 'profile)))))
  (let* ((surface
        (module defaults-test
          (use-profile default-profile "1.0")
          (actor user :kind person)
          (type Id :opaque Text)
          (component app :responsibility "test" :provides (api))
          (interface api
            (command do :actor user :input Id :output Id
              :errors (forbidden)))
          (state items :of (List Id) :owner app :durability durable)
          (flow f :from user :to api :kind command :grant (auth))
          (behavior do :on (api/do user item)
            :reads (items) :writes (items) :atomic items
            (requires (ok? user)) (ensures (ok? post)))
          (synthesis s :target (lamedh :track "0.5")
            :model (small-code-model :class nano :temperature 0))
          (acceptance t :subject app
            (property p :generate ((x g))
              :execute (do x) :must (ok? result)))))
      (ir (gymnast-elaborate surface)))
    (assert-false (gymnast-has-errors-p (gymnast-ir-field ir 'diagnostics)))
    (assert-true (gymnast-find-ir-node ir
        "defaults-test/constraint/profile-constraint"))))

;;; Canonical serialization contract tests.

(deftest canonical-ir-passes-validation
  (let ((ir (gymnast-elaborate gymnast-test-spec)))
    (assert-equal (gymnast-validate-ir-canonical ir) nil)))

(deftest canonical-plan-passes-validation
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir)))
    (assert-equal (gymnast-validate-plan-canonical plan) nil)))

(deftest plan-constructor-enforces-sorted-inputs
  (let ((node (gymnast-plan-node "test/plan/sorted" 'structural 'test-v1
          '("z-input" "a-input" "m-input")
          '("z-dep" "a-dep")
          '(lamedh) '(none) '("z.lisp" "a.lisp") nil nil nil)))
    (assert-true (gymnast-strings-canonical-p
        (gymnast-plan-node-field node 'inputs)))
    (assert-true (gymnast-strings-canonical-p
        (gymnast-plan-node-field node 'depends-on)))
    (assert-true (gymnast-strings-canonical-p
        (gymnast-plan-node-field node 'may-write)))))

(deftest serialization-is-byte-stable-across-calls
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (a (gymnast-canonical-serialize ir))
      (b (gymnast-canonical-serialize ir)))
    (assert-equal a b)))

(deftest schema-versions-present-in-all-artifacts
  (let* ((compilation (gymnast-compile gymnast-test-spec))
      (ir (gymnast-compilation-field compilation 'ir))
      (plan (gymnast-compilation-field compilation 'plan)))
    (assert-equal (gymnast-ir-field ir 'schema) $gymnast-ir-schema)
    (assert-equal (gymnast-plan-field plan 'schema) $gymnast-plan-schema)))

(deftest ir-fingerprint-is-verifiable
  (let ((ir (gymnast-elaborate gymnast-test-spec)))
    (assert-equal (gymnast-verify-fingerprint ir 'ir) nil)))

(deftest plan-fingerprint-is-verifiable
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir)))
    (assert-equal (gymnast-verify-fingerprint plan 'plan) nil)))

;;; Executable transition calculus tests.

(deftest transition-extraction-from-behavior-nodes
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (transitions (gymnast-extract-transitions ir)))
    (assert-equal (length transitions) 1)
    (let ((tr (car transitions)))
      (assert-equal (gymnast-transition-field tr 'operation) "api/add")
      (assert-equal (gymnast-transition-field tr 'actor) 'user)
      (assert-equal (gymnast-transition-field tr 'input) 'item)
      (assert-true (member 'items (gymnast-transition-field tr 'reads)))
      (assert-true (member 'items (gymnast-transition-field tr 'writes)))
      (assert-equal (gymnast-transition-field tr 'atomic) 'items)
      (assert-equal (gymnast-transition-field tr 'idempotency) 'command-key)
      (assert-equal (length (gymnast-transition-field tr 'preconditions)) 1)
      (assert-equal (length (gymnast-transition-field tr 'postconditions)) 1))))

(deftest transition-reference-checking-valid-refs
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (transitions (gymnast-extract-transitions ir))
      (diagnostics (gymnast-check-all-transitions ir transitions)))
    (assert-equal (length diagnostics) 0)))

(deftest transition-reference-checking-invalid-refs
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (bad-transition
        (list 'transition
          (list 'id "test-bad")
          (list 'operation "api/add")
          (list 'actor nil)
          (list 'input nil)
          (list 'reads (list 'nonexistent-state))
          (list 'writes nil)
          (list 'atomic nil)
          (list 'idempotency nil)
          (list 'preconditions nil)
          (list 'postconditions nil)
          (list 'result nil)
          (list 'failures nil)
          (list 'emissions nil)))
      (diagnostics (gymnast-check-all-transitions ir (list bad-transition))))
    (assert-equal (length diagnostics) 1)))

(deftest initial-state-construction
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (state (gymnast-make-initial-state ir)))
    (assert-true (> (length state) 0))
    (assert-true (assoc 'items state))))

(deftest predicate-evaluation-equality
  (let ((state (list (list 'x 42))))
    (assert-true (gymnast-eval-predicate '(= x 42) state nil nil))
    (assert-false (gymnast-eval-predicate '(= x 99) state nil nil))))

(deftest predicate-evaluation-logical-ops
  (let ((state (list (list 'x 42))))
    (assert-true (gymnast-eval-predicate '(and (= x 42)) state nil nil))
    (assert-false (gymnast-eval-predicate '(and (= x 42) (= x 99))
        state nil nil))
    (assert-true (gymnast-eval-predicate '(or (= x 99) (= x 42))
        state nil nil))
    (assert-true (gymnast-eval-predicate '(not (= x 99)) state nil nil))
    (assert-false (gymnast-eval-predicate '(not (= x 42)) state nil nil))))

(deftest predicate-evaluation-comparison
  (let ((state (list (list 'x 10))))
    (assert-true (gymnast-eval-predicate '(< x 20) state nil nil))
    (assert-false (gymnast-eval-predicate '(< x 5) state nil nil))
    (assert-true (gymnast-eval-predicate '(<= x 10) state nil nil))
    (assert-false (gymnast-eval-predicate '(<= x 9) state nil nil))))

(deftest trace-execution-with-test-spec
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (trace (gymnast-execute-trace ir
          (list (list "api/add" 'user "item-1")))))
    (assert-true (gymnast-tagged-p 'trace trace))
    (let ((steps (gymnast-trace-field trace 'steps))
        (final (gymnast-trace-field trace 'final-state)))
      (assert-equal (length steps) 1)
      (assert-true (consp final)))))

(deftest trace-unknown-operation-produces-violation
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (trace (gymnast-execute-trace ir
          (list (list "api/delete" 'user "item-1")))))
    (assert-true (> (length (gymnast-trace-violations trace)) 0))))

(deftest counterexample-structure
  (let* ((violation (list 'violation (list 'type 'test) (list 'detail "x")))
      (step (list 'trace-step
          (list 'transition-id "t1")
          (list 'actor 'user)
          (list 'input "data")
          (list 'pre-state (list (list 'x 1)))
          (list 'post-state (list (list 'x 1)))
          (list 'result nil)
          (list 'outcome (list 'failed 'error))))
      (ce (gymnast-counterexample violation step)))
    (assert-true (gymnast-tagged-p 'counterexample ce))
    (assert-equal (gymnast-assoc-value 'violation (cdr ce)) violation)
    (assert-equal (gymnast-assoc-value 'input (cdr ce)) "data")))

;;; Platform kit tests.

(deftest ruby-platform-kit-is-registered
  (assert-true $gymnast-ruby-platform-kit)
  (assert-equal
    (gymnast-platform-kit-field $gymnast-ruby-platform-kit 'name)
    'gymnast-ruby-platform-v1)
  (assert-equal
    (gymnast-platform-kit-field $gymnast-ruby-platform-kit 'target)
    'ruby))

(deftest ruby-platform-kit-has-all-capabilities
  (let ((names (gymnast-platform-kit-capability-names
          $gymnast-ruby-platform-kit)))
    (assert-true (member 'identity names))
    (assert-true (member 'persistence names))
    (assert-true (member 'repository names))
    (assert-true (member 'transactions names))
    (assert-true (member 'clock names))
    (assert-true (member 'id-source names))
    (assert-true (member 'http names))
    (assert-true (member 'telemetry names))
    (assert-true (member 'lifecycle names))
    (assert-true (member 'durable-store names))))

(deftest platform-kit-lookup-by-version
  (let ((kit (gymnast-lookup-platform-kit
          'gymnast-ruby-platform-v1 "1.0")))
    (assert-true kit)
    (assert-equal (gymnast-platform-kit-field kit 'version) "1.0")))

(deftest platform-capability-has-characterization
  (let* ((caps (gymnast-platform-kit-field
          $gymnast-ruby-platform-kit 'capabilities))
      (clock-cap (car (filter
            (lambda (c)
              (equal (gymnast-capability-field c 'name) 'clock))
            caps))))
    (assert-true clock-cap)
    (assert-true (member 'monotonic
        (gymnast-capability-field clock-cap 'guarantees)))
    (assert-true (member 'drift-beyond-tolerance
        (gymnast-capability-field clock-cap 'failure-modes)))))

(deftest platform-validates-known-capabilities
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (diagnostics (gymnast-validate-plan-capabilities
          plan $gymnast-ruby-platform-kit)))
    (assert-equal (length diagnostics) 0)))

;;; Recipe registry and executor tests.

(deftest recipe-registry-has-all-plan-recipes
  (assert-true (gymnast-lookup-recipe 'design-contracts-v1))
  (assert-true (gymnast-lookup-recipe 'interface-contracts-v1))
  (assert-true (gymnast-lookup-recipe 'acceptance-harness-v1))
  (assert-true (gymnast-lookup-recipe 'application-assembly-v1))
  (assert-true (gymnast-lookup-recipe 'transition-kernel-v1))
  (assert-true (gymnast-lookup-recipe 'authorization-policy-v1))
  (assert-true (gymnast-lookup-recipe 'persistence-v1))
  (assert-true (gymnast-lookup-recipe 'service-handlers-v1)))

(deftest unknown-recipe-fails-closed
  (assert-false (gymnast-lookup-recipe 'nonexistent-recipe-v1)))

(deftest structural-recipes-are-deterministic
  (let* ((r (gymnast-lookup-recipe 'design-contracts-v1)))
    (assert-false (gymnast-recipe-requires-model-p r))))

(deftest generative-recipes-require-model
  (let* ((r (gymnast-lookup-recipe 'transition-kernel-v1)))
    (assert-true (gymnast-recipe-requires-model-p r))))

(deftest deterministic-execution-produces-results
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (results (gymnast-execute-deterministic ir plan)))
    (assert-equal (length results) 8)
    (let ((deterministic (gymnast-deterministic-results results))
        (deferred (gymnast-deferred-results results)))
      (assert-equal (length deterministic) 4)
      (assert-equal (length deferred) 4))))

(deftest structural-recipe-produces-valid-candidate
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (gymnast-find-plan-node plan
          (gymnast-plan-id ir "design-contracts")))
      (result (gymnast-execute-recipe ir plan node)))
    (assert-equal
      (gymnast-execution-result-field result 'status) 'succeeded)
    (assert-equal
      (gymnast-execution-result-field result 'recipe-identity)
      'design-contracts-v1)))

(deftest structural-recipe-output-is-byte-stable
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (gymnast-find-plan-node plan
          (gymnast-plan-id ir "design-contracts")))
      (a (gymnast-execute-recipe ir plan node))
      (b (gymnast-execute-recipe ir plan node)))
    (assert-equal
      (gymnast-execution-result-field a 'candidate)
      (gymnast-execution-result-field b 'candidate))))

(deftest generative-recipe-defers-to-model
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (gymnast-find-plan-node plan
          (gymnast-plan-id ir "transition-kernel")))
      (result (gymnast-execute-recipe ir plan node)))
    (assert-equal
      (gymnast-execution-result-field result 'status) 'deferred)
    (assert-equal
      (gymnast-execution-result-field result 'reason) 'requires-model)))
