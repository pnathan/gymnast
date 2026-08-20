(include "../src/gymnast.lisp")

(defspec gymnast-test-spec
  :version "test"
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
      :must (contains? result item))
    (scenario add-and-verify
      (given user (authenticated-user))
      (when (add user new-item))
      (then succeeds)
      (when (list user))
      (then (contains? result new-item)))
    (concurrency concurrent-adds
      :actors 10
      :schedule adversarial
      :must (= lost-updates 0))
    (fault restart-durability
      :after acknowledged-write
      :inject restart
      :must (read-your-acknowledged-write))
    (coverage
      :every-operation true
      :every-error true
      :every-transition true
      :every-invariant true
      :boundaries true)
    (execution
      :clock virtual
      :randomness seeded
      :network controlled
      :timezone "UTC")))

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
  (let ((lowered (use-profile some-profile "1.0")))
    (assert-equal (gymnast-surface-kind lowered) 'import)
    (assert-equal (gymnast-surface-mechanism lowered) 'fexpr)))

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

(deftest unknown-profile-is-error
  (let* ((surface
        (module unknown-test
          (use-profile nonexistent-profile "1.0")
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
      (ir (gymnast-elaborate surface))
      (diagnostics (gymnast-ir-field ir 'diagnostics)))
    (assert-true (gymnast-has-errors-p diagnostics))
    (assert-true (gymnast-any
        (lambda (d)
          (equal (gymnast-diagnostic-field d 'code) 'unknown-profile))
        diagnostics))))

;;; Port declaration tests.

(defspec port-test-spec
  :version "test"
  (actor user :kind person)
  (type UserId :opaque Text)
  (type ChargeId :opaque Text)
  (type Charge :record ((id ChargeId) (amount Integer) (currency Text)))
  (type User :record ((id UserId) (name Text)))
  (component app :responsibility "Payment service" :provides (api))
  (interface api
    (command create-charge :actor user :input Charge :output Charge
      :errors (forbidden)))
  (port payment-api
    :direction provides
    :protocol rest
    :content-type json
    (operation create-charge :method POST :path "/charges")
    (operation get-charge :method GET :path "/charges/:id"))
  (port user-service
    :direction requires
    :protocol grpc
    :content-type protobuf
    (operation get-user :returns User)
    (operation list-users :returns (List User)))
  (state charges :of (List Charge) :owner app :durability durable)
  (flow access :from user :to api :kind command :grant (authenticated))
  (behavior create-charge
    :on (api/create-charge user charge)
    :reads (charges) :writes (charges) :atomic charges
    (requires (authenticated? user))
    (ensures (contains? post charge)))
  (constraint load :class workload :must (supports 100 concurrent-users))
  (synthesis test
    :target (lamedh :track "0.5")
    :model (small-code-model :class nano :temperature 0))
  (acceptance test :subject app
    (property round-trip
      :generate ((charge valid-charge))
      :execute (create-charge charge) :must (ok? result))))

(deftest port-elaborates-into-design-graph
  (let* ((ir (gymnast-elaborate port-test-spec))
      (ports (gymnast-ir-nodes-of-kind ir 'port)))
    (assert-false (gymnast-has-errors-p (gymnast-ir-field ir 'diagnostics)))
    (assert-equal (length ports) 2)
    (assert-true (gymnast-find-ir-node ir
        "port-test-spec/port/payment-api"))
    (assert-true (gymnast-find-ir-node ir
        "port-test-spec/port/user-service"))))

(deftest port-direction-and-protocol-are-required
  (let* ((surface
        (module port-missing
          (port bad-port :protocol rest)))
      (ir (gymnast-elaborate surface)))
    (assert-true (gymnast-has-errors-p (gymnast-ir-field ir 'diagnostics)))))

(deftest port-carries-protocol-and-direction
  (let* ((ir (gymnast-elaborate port-test-spec))
      (node (gymnast-find-ir-node ir "port-test-spec/port/payment-api"))
      (fields (gymnast-ir-node-field node 'fields)))
    (assert-equal (gymnast-assoc-value ':direction fields) 'provides)
    (assert-equal (gymnast-assoc-value ':protocol fields) 'rest)
    (assert-equal (gymnast-assoc-value ':content-type fields) 'json)))

(deftest port-operations-captured-as-clauses
  (let* ((ir (gymnast-elaborate port-test-spec))
      (node (gymnast-find-ir-node ir "port-test-spec/port/payment-api"))
      (clauses (gymnast-ir-node-field node 'clauses)))
    (assert-equal (length clauses) 2)
    (assert-equal (car (car clauses)) 'operation)))

(deftest port-nodes-flow-into-plan
  (let* ((ir (gymnast-elaborate port-test-spec))
      (plan (gymnast-plan ir))
      (interface-node (gymnast-find-plan-node plan
          (gymnast-plan-id ir "interface-contracts")))
      (handler-node (gymnast-find-plan-node plan
          (gymnast-plan-id ir "service-handlers")))
      (port-id "port-test-spec/port/payment-api"))
    (assert-true (member port-id
        (gymnast-plan-node-field interface-node 'inputs)))
    (assert-true (member port-id
        (gymnast-plan-node-field handler-node 'inputs)))))

(deftest port-projected-into-prompts
  (let* ((ir (gymnast-elaborate port-test-spec))
      (plan (gymnast-plan ir))
      (prompts (gymnast-compile-prompts ir plan))
      (interface-prompt (car
          (filter
            (lambda (p)
              (gymnast-string-contains
                (gymnast-assoc-value 'node-id (cdr p))
                "interface-contracts"))
            prompts)))
      (text (gymnast-assoc-value 'text (cdr interface-prompt))))
    (assert-true (gymnast-string-contains text "PORT BOUNDARIES"))
    (assert-true (gymnast-string-contains text "payment-api"))))

(deftest port-spec-compiles-reproducibly
  (let ((a (gymnast-compile port-test-spec))
      (b (gymnast-compile port-test-spec)))
    (assert-equal
      (gymnast-compilation-field a 'fingerprint)
      (gymnast-compilation-field b 'fingerprint))))

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
        (make-gymnast-transition "test-bad" "api/add" nil nil
          (list 'nonexistent-state) nil nil nil nil nil nil nil nil))
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
      (step (make-gymnast-trace-step "t1" 'user "data"
          (list (list 'x 1)) (list (list 'x 1)) nil (list 'failed 'error)))
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

(deftest platform-capabilities-lookup-by-target
  (let ((caps (gymnast-platform-capabilities-for-target 'ruby)))
    (assert-true caps)
    (assert-true (> (length caps) 0))
    (assert-true
      (gymnast-any
        (lambda (c) (equal (gymnast-capability-field c 'name) 'identity))
        caps))))

(deftest platform-capabilities-lookup-cons-target
  (let ((caps (gymnast-platform-capabilities-for-target
          '(ruby :framework rails))))
    (assert-true caps)
    (assert-true (> (length caps) 0))))

(deftest platform-capabilities-missing-target-returns-nil
  (assert-false (gymnast-platform-capabilities-for-target 'haskell)))

(deftest prompt-text-includes-capability-contracts
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (nodes (gymnast-plan-field plan 'nodes))
      (generative (car (filter
            (lambda (n)
              (equal (gymnast-plan-node-field n 'class) 'generative))
            nodes)))
      (prompt (gymnast-compile-prompt ir plan generative))
      (text (gymnast-assoc-value 'text (cdr prompt))))
    (assert-true (gymnast-string-contains text "CAPABILITY CONTRACTS"))))

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

;;; Model runner tests.

(defun make-stub-provider (candidate-string)
  (lambda (request) candidate-string))

(deftest model-request-preparation
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (gymnast-find-plan-node plan
          (gymnast-plan-id ir "transition-kernel")))
      (prompt (gymnast-compile-prompt ir plan node))
      (request (gymnast-prepare-model-request prompt)))
    (assert-true (gymnast-tagged-p 'model-request request))
    (assert-true (stringp (gymnast-model-request-field request 'prompt-text)))
    (assert-true (gymnast-model-request-field request 'prompt-fingerprint))))

(deftest runner-accepts-valid-candidate
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (gymnast-find-plan-node plan
          (gymnast-plan-id ir "transition-kernel")))
      (files (gymnast-plan-node-field node 'may-write))
      (candidate-text (prin1-to-string
          (list 'candidate
            (list 'schema $gymnast-candidate-schema)
            (list 'node-id (gymnast-plan-node-id node))
            (list 'files
              (list (list (car files) "# generated code")))
            (list 'implements nil)
            (list 'edge-uses nil)
            (list 'assumptions nil)
            (list 'unresolved nil))))
      (provider (make-stub-provider candidate-text))
      (result (gymnast-run-node ir plan node provider 3)))
    (assert-equal (gymnast-run-result-field result 'status) 'succeeded)
    (assert-equal (length (gymnast-run-result-field result 'attempts)) 1)))

(deftest runner-rejects-and-retries-bad-candidate
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (gymnast-find-plan-node plan
          (gymnast-plan-id ir "transition-kernel")))
      (provider (make-stub-provider "not-an-sexpr"))
      (result (gymnast-run-node ir plan node provider 2)))
    (assert-equal (gymnast-run-result-field result 'status) 'exhausted)
    (assert-equal (length (gymnast-run-result-field result 'attempts)) 2)))

(deftest runner-records-attempt-provenance
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (gymnast-find-plan-node plan
          (gymnast-plan-id ir "transition-kernel")))
      (provider (make-stub-provider "garbage"))
      (result (gymnast-run-node ir plan node provider 1))
      (attempts (gymnast-run-result-field result 'attempts))
      (attempt (car attempts)))
    (assert-equal (gymnast-attempt-field attempt 'number) 1)
    (assert-equal (gymnast-attempt-field attempt 'status) 'rejected)
    (assert-true (gymnast-attempt-field attempt 'prompt-fingerprint))))

(deftest safe-read-parses-valid-sexpr
  (let ((result (gymnast-safe-read "(candidate (node-id \"x\"))")))
    (assert-true (gymnast-tagged-p 'candidate result))
    (assert-equal (gymnast-assoc-value 'node-id (cdr result)) "x")))

(deftest safe-read-handles-empty-input
  (assert-equal (gymnast-safe-read "") nil)
  (assert-equal (gymnast-safe-read nil) nil))

;;; Verification obligation tests.

(deftest obligation-lowering-extracts-all-clause-types
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (obligations (gymnast-lower-all-obligations ir)))
    (assert-true (> (length obligations) 0))
    (let ((kinds (mapcar
            (lambda (ob) (gymnast-obligation-field ob 'kind))
            obligations)))
      (assert-true (member 'property kinds))
      (assert-true (member 'scenario kinds))
      (assert-true (member 'concurrency kinds))
      (assert-true (member 'fault kinds))
      (assert-true (member 'coverage kinds))
      (assert-true (member 'invariant kinds))
      (assert-true (member 'constraint kinds)))))

(deftest obligation-lowering-produces-correct-counts
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (obligations (gymnast-lower-all-obligations ir)))
    (let ((acceptance-obs (gymnast-lower-acceptance-obligations ir))
        (invariant-obs (gymnast-lower-invariant-obligations ir))
        (constraint-obs (gymnast-lower-constraint-obligations ir)))
      (assert-equal (length acceptance-obs) 5)
      (assert-equal (length invariant-obs) 1)
      (assert-equal (length constraint-obs) 1)
      (assert-equal (length obligations) 7))))

(deftest execution-environment-extraction
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (acc-nodes (gymnast-ir-nodes-of-kind ir 'acceptance))
      (env (gymnast-extract-execution-env (car acc-nodes))))
    (assert-equal (gymnast-env-field env 'clock) 'virtual)
    (assert-equal (gymnast-env-field env 'randomness) 'seeded)
    (assert-equal (gymnast-env-field env 'network) 'controlled)
    (assert-equal (gymnast-env-field env 'timezone) "UTC")
    (assert-true (gymnast-env-deterministic-p env))))

(deftest non-deterministic-env-produces-warnings
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (env (list 'execution-environment
          (list 'clock 'system)
          (list 'randomness 'system)
          (list 'network 'system)
          (list 'locale "en-US")
          (list 'timezone "UTC")))
      (diags (gymnast-env-diagnostics env "test-acceptance")))
    (assert-equal (length diags) 3)))

(deftest property-obligation-has-execute-and-assertion
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (obligations (gymnast-lower-acceptance-obligations ir))
      (prop (car (filter
            (lambda (ob)
              (equal (gymnast-obligation-field ob 'kind) 'property))
            obligations))))
    (assert-true prop)
    (assert-true (gymnast-obligation-field prop 'execute))
    (assert-true (gymnast-obligation-field prop 'assertion))
    (assert-equal (gymnast-obligation-field prop 'name) 'round-trip)))

(deftest scenario-obligation-has-steps
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (obligations (gymnast-lower-acceptance-obligations ir))
      (scenario (car (filter
            (lambda (ob)
              (equal (gymnast-obligation-field ob 'kind) 'scenario))
            obligations))))
    (assert-true scenario)
    (assert-true (gymnast-obligation-field scenario 'steps))
    (assert-equal (gymnast-obligation-field scenario 'name) 'add-and-verify)))

(deftest concurrency-obligation-has-actor-count
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (obligations (gymnast-lower-acceptance-obligations ir))
      (conc (car (filter
            (lambda (ob)
              (equal (gymnast-obligation-field ob 'kind) 'concurrency))
            obligations))))
    (assert-true conc)
    (assert-equal (gymnast-obligation-field conc 'actors) 10)
    (assert-equal (gymnast-obligation-field conc 'schedule) 'adversarial)))

(deftest fault-obligation-has-injection-spec
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (obligations (gymnast-lower-acceptance-obligations ir))
      (fault (car (filter
            (lambda (ob)
              (equal (gymnast-obligation-field ob 'kind) 'fault))
            obligations))))
    (assert-true fault)
    (assert-equal (gymnast-obligation-field fault 'inject) 'restart)
    (assert-equal (gymnast-obligation-field fault 'after) 'acknowledged-write)))

(deftest invariant-obligation-checks-initial-state
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (obligations (gymnast-lower-invariant-obligations ir))
      (inv (car obligations)))
    (assert-true inv)
    (assert-equal (gymnast-obligation-field inv 'kind) 'invariant)
    (let ((result (gymnast-verify-invariant-obligation ir inv)))
      (assert-equal
        (gymnast-verification-result-field result 'status) 'passed))))

(deftest invariant-checks-post-transition-states
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (inv-ob (list 'verification-obligation
          (list 'id "test/invariant/empty-items/invariant-check")
          (list 'kind 'invariant)
          (list 'source "test/invariant/empty-items")
          (list 'name 'empty-items)
          (list 'scope 'items)
          (list 'predicate '(not (= items (nil))))
          (list 'environment nil)))
      (result (gymnast-verify-invariant-obligation ir inv-ob)))
    (assert-equal
      (gymnast-verification-result-field result 'status) 'failed)
    (let* ((cxs (gymnast-verification-result-field
            result 'counterexamples))
        (cx (car cxs)))
      (assert-true cx)
      (assert-equal
        (gymnast-assoc-value 'divergence-type (cdr cx))
        'invariant-violation-post-transition))))

(deftest verification-bundle-compiles
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (bundle (gymnast-compile-verification ir)))
    (assert-true (gymnast-tagged-p 'verification-bundle bundle))
    (assert-equal
      (gymnast-verification-bundle-field bundle 'schema)
      $gymnast-verify-schema)
    (let ((summary (gymnast-verification-bundle-field bundle 'summary)))
      (assert-true (> (gymnast-assoc-value 'total summary) 0)))))

(deftest trace-equivalence-detects-matching-traces
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (steps (list (list "api/add" 'user 'item1)))
      (trace-a (gymnast-execute-trace ir steps))
      (trace-b (gymnast-execute-trace ir steps)))
    (assert-true (gymnast-trace-equivalent-p trace-a trace-b))))

(deftest trace-equivalence-detects-divergence
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (trace-a (gymnast-execute-trace ir
          (list (list "api/add" 'user 'item1))))
      (trace-b (gymnast-execute-trace ir
          (list (list "api/add" 'user 'item2)))))
    (assert-false (gymnast-trace-equivalent-p trace-a trace-b))))

(deftest normalized-counterexample-has-required-fields
  (let ((div (list 'divergence
          (list 'type 'outcome-mismatch)
          (list 'reference '(succeeded))
          (list 'implementation '(failed forbidden))
          (list 'step (make-gymnast-trace-step "api/add" 'user 'item
              nil nil nil '(failed forbidden))))))
    (let ((cx (gymnast-normalize-counterexample div "test-ob")))
      (assert-true (gymnast-tagged-p 'normalized-counterexample cx))
      (assert-equal
        (gymnast-assoc-value 'obligation-id (cdr cx)) "test-ob")
      (assert-equal
        (gymnast-assoc-value 'operation (cdr cx)) "api/add")
      (assert-equal
        (gymnast-assoc-value 'divergence-type (cdr cx)) 'outcome-mismatch))))

(deftest coverage-analysis-counts-obligations
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (obligations (gymnast-lower-all-obligations ir))
      (coverage (gymnast-coverage-gaps ir obligations)))
    (assert-true coverage)
    (assert-true (gymnast-tagged-p 'coverage-analysis coverage))
    (assert-true (>= (gymnast-assoc-value 'property-obligations
          (cdr coverage)) 1))
    (assert-true (>= (gymnast-assoc-value 'scenario-obligations
          (cdr coverage)) 1))))

;;; Content-addressed caching tests.

(deftest cache-key-is-deterministic
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (car (gymnast-plan-field plan 'nodes)))
      (key-a (gymnast-cache-key ir plan node))
      (key-b (gymnast-cache-key ir plan node)))
    (assert-equal key-a key-b)
    (assert-true (stringp key-a))))

(deftest cache-store-and-lookup
  (gymnast-cache-clear)
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (car (gymnast-plan-field plan 'nodes)))
      (key (gymnast-cache-key ir plan node))
      (entry (gymnast-cache-entry key
          (gymnast-plan-node-id node)
          '(candidate (dummy true))
          '(evidence (dummy true))
          'now)))
    (gymnast-cache-store key entry)
    (assert-equal (gymnast-cache-size) 1)
    (assert-true (gymnast-cache-lookup key))
    (assert-equal
      (gymnast-cache-entry-field (gymnast-cache-lookup key) 'node-id)
      (gymnast-plan-node-id node))
    (gymnast-cache-clear)))

(deftest cache-miss-returns-nil
  (gymnast-cache-clear)
  (assert-equal (gymnast-cache-lookup "nonexistent-key") nil)
  (assert-equal (gymnast-cache-size) 0))

(deftest cache-validity-checks-key-match
  (gymnast-cache-clear)
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (car (gymnast-plan-field plan 'nodes)))
      (key (gymnast-cache-key ir plan node))
      (entry (gymnast-cache-entry key
          (gymnast-plan-node-id node) nil nil 'now)))
    (gymnast-cache-store key entry)
    (assert-true (gymnast-cache-entry-valid-p ir plan node
        (gymnast-cache-lookup key)))
    (let ((wrong-entry (gymnast-cache-entry "wrong-key"
            (gymnast-plan-node-id node) nil nil 'now)))
      (assert-false (gymnast-cache-entry-valid-p ir plan node wrong-entry)))
    (gymnast-cache-clear)))

(deftest cache-check-plan-reports-hits-and-misses
  (gymnast-cache-clear)
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (results (gymnast-cache-check-plan ir plan)))
    (assert-equal (length results) 8)
    (assert-equal (length (gymnast-cache-hits results)) 0)
    (assert-equal (length (gymnast-cache-misses results)) 8))
  (gymnast-cache-clear))

(deftest cache-hit-after-store
  (gymnast-cache-clear)
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (car (gymnast-plan-field plan 'nodes))))
    (gymnast-cache-store-result ir plan node
      '(candidate (test true)) '(evidence (test true)))
    (let ((results (gymnast-cache-check-plan ir plan)))
      (assert-equal (length (gymnast-cache-hits results)) 1)
      (assert-equal (length (gymnast-cache-misses results)) 7)))
  (gymnast-cache-clear))

(deftest dependency-closure-computation
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (design-id (gymnast-plan-id ir "design-contracts"))
      (affected (gymnast-invalidated-nodes plan (list design-id))))
    (assert-true (member design-id affected))
    (assert-true (> (length affected) 1))))

(deftest plan-diff-detects-unchanged
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan-a (gymnast-plan ir))
      (plan-b (gymnast-plan ir))
      (diff (gymnast-diff-plans plan-a plan-b)))
    (assert-equal (length (gymnast-diff-field diff 'modified)) 0)
    (assert-equal (length (gymnast-diff-field diff 'added)) 0)
    (assert-equal (length (gymnast-diff-field diff 'removed)) 0)
    (assert-equal (length (gymnast-diff-field diff 'unchanged)) 8)))

(deftest cache-explain-reports-miss-reason
  (gymnast-cache-clear)
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (car (gymnast-plan-field plan 'nodes)))
      (explanation (gymnast-cache-explain-node ir plan node)))
    (assert-equal
      (gymnast-assoc-value 'status (cdr explanation)) 'miss)
    (assert-equal
      (gymnast-assoc-value 'reason (cdr explanation)) 'no-cache-entry))
  (gymnast-cache-clear))

(deftest cache-explain-reports-hit-after-store
  (gymnast-cache-clear)
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (node (car (gymnast-plan-field plan 'nodes))))
    (gymnast-cache-store-result ir plan node
      '(candidate (test true)) '(evidence (test true)))
    (let ((explanation (gymnast-cache-explain-node ir plan node)))
      (assert-equal
        (gymnast-assoc-value 'status (cdr explanation)) 'hit)
      (assert-equal
        (gymnast-assoc-value 'reason (cdr explanation)) 'valid-entry)))
  (gymnast-cache-clear))

(deftest identical-compile-produces-no-model-calls-with-cache
  (gymnast-cache-clear)
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir)))
    (dolist (node (gymnast-plan-field plan 'nodes))
      (gymnast-cache-store-result ir plan node
        '(candidate (cached true)) '(evidence (cached true))))
    (let ((results (gymnast-cache-check-plan ir plan)))
      (assert-equal (length (gymnast-cache-hits results)) 8)
      (assert-equal (length (gymnast-cache-misses results)) 0)))
  (gymnast-cache-clear))

;;; Assembly and evidence bundle tests.

(defun make-test-execution-results (ir plan)
  (mapcar
    (lambda (node)
      (let* ((node-id (gymnast-plan-node-id node))
          (files (gymnast-plan-node-field node 'may-write)))
        (list 'run-result
          (list 'node-id node-id)
          (list 'status 'succeeded)
          (list 'candidate
            (list 'candidate
              (list 'schema $gymnast-candidate-schema)
              (list 'node-id node-id)
              (list 'files
                (mapcar (lambda (f) (list f "# generated")) files))
              (list 'implements nil)
              (list 'edge-uses nil)
              (list 'assumptions nil)
              (list 'unresolved nil))))))
    (gymnast-plan-field plan 'nodes)))

(deftest artifact-collection-from-results
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (results (make-test-execution-results ir plan))
      (artifacts (gymnast-collect-artifacts results)))
    (assert-true (> (length artifacts) 0))
    (let ((first (car artifacts)))
      (assert-true (gymnast-artifact-p first))
      (assert-true (gymnast-artifact-field first 'path))
      (assert-true (gymnast-artifact-field first 'digest))
      (assert-true (gymnast-artifact-field first 'node-id)))))

(deftest artifact-validation-accepts-declared-paths
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (results (make-test-execution-results ir plan))
      (artifacts (gymnast-collect-artifacts results))
      (diags (gymnast-validate-artifacts plan artifacts)))
    (let ((errors (filter #'gymnast-error-diagnostic-p diags)))
      (assert-equal (length errors) 0))))

(deftest capability-edge-validation
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (diags (gymnast-validate-capability-edges plan)))
    (assert-equal (length diags) 0)))

(deftest traceability-map-covers-all-ir-nodes
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (results (make-test-execution-results ir plan))
      (traceability (gymnast-build-traceability-map ir plan results)))
    (assert-equal (length traceability)
      (length (gymnast-ir-all-nodes ir)))
    (let ((first (car traceability)))
      (assert-true (gymnast-traceability-entry-p first))
      (assert-true
        (gymnast-traceability-entry-field first 'semantic-id)))))

(deftest dependency-lock-captures-plan-state
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (lock (gymnast-dependency-lock plan)))
    (assert-true (gymnast-tagged-p 'dependency-lock lock))
    (assert-equal
      (gymnast-dependency-lock-field lock 'plan-fingerprint)
      (gymnast-plan-field plan 'fingerprint))
    (assert-equal
      (length (gymnast-dependency-lock-field lock 'node-locks)) 8)))

(deftest evidence-bundle-assembles
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (results (make-test-execution-results ir plan))
      (verification (gymnast-compile-verification ir))
      (bundle (gymnast-assemble-bundle ir plan results verification)))
    (assert-true (gymnast-tagged-p 'evidence-bundle bundle))
    (assert-equal
      (gymnast-bundle-field bundle 'schema) $gymnast-bundle-schema)
    (assert-true (gymnast-bundle-field bundle 'artifacts))
    (assert-true (gymnast-bundle-field bundle 'traceability))
    (assert-true (gymnast-bundle-field bundle 'dependency-lock))
    (assert-true (gymnast-bundle-field bundle 'verification))
    (let ((summary (gymnast-bundle-field bundle 'summary)))
      (assert-equal (gymnast-assoc-value 'total-nodes summary) 8)
      (assert-equal (gymnast-assoc-value 'failed-nodes summary) 0))))

(deftest promotion-policy-evaluates-clean-bundle
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (results (make-test-execution-results ir plan))
      (bundle (gymnast-assemble-bundle ir plan results nil))
      (policy (gymnast-default-promotion-policy))
      (result (gymnast-evaluate-promotion policy bundle)))
    (assert-true (gymnast-tagged-p 'promotion-result result))
    (assert-equal
      (gymnast-promotion-result-field result 'decision) 'promote)))

(deftest promotion-holds-on-failed-verification
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (results (make-test-execution-results ir plan))
      (verification (gymnast-compile-verification ir))
      (bundle (gymnast-assemble-bundle ir plan results verification))
      (policy (gymnast-default-promotion-policy))
      (result (gymnast-evaluate-promotion policy bundle))
      (checks (gymnast-promotion-result-field result 'checks)))
    (assert-equal
      (gymnast-promotion-result-field result 'decision) 'hold)
    (assert-equal (cadr (assoc 'verification-passed checks)) nil)))

(deftest promotion-holds-on-failed-nodes
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (plan (gymnast-plan ir))
      (results (list (list 'run-result
            (list 'node-id "test")
            (list 'status 'failed)
            (list 'candidate nil))))
      (bundle (gymnast-assemble-bundle ir plan results nil))
      (policy (gymnast-default-promotion-policy))
      (result (gymnast-evaluate-promotion policy bundle)))
    (assert-equal
      (gymnast-promotion-result-field result 'decision) 'hold)))

;;; Adequacy campaign tests.

(deftest mutant-weaken-precondition-changes-ir
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (mutated (gymnast-mutate-weaken-precondition ir 'add))
      (behaviors (gymnast-ir-nodes-of-kind mutated 'behavior))
      (add-behavior (car behaviors))
      (clauses (gymnast-ir-node-field add-behavior 'clauses))
      (requires (filter
          (lambda (c) (equal (gymnast-clause-head c) 'requires))
          clauses)))
    (assert-equal (length requires) 0)))

(deftest mutant-remove-invariant-drops-node
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (before (length (gymnast-ir-nodes-of-kind ir 'invariant)))
      (mutated (gymnast-mutate-remove-invariant ir 'no-duplicates))
      (after (length (gymnast-ir-nodes-of-kind mutated 'invariant))))
    (assert-equal after (- before 1))))

(deftest mutant-skip-state-write-clears-writes
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (mutated (gymnast-mutate-skip-state-write ir 'add))
      (behaviors (gymnast-ir-nodes-of-kind mutated 'behavior))
      (add-behavior (car behaviors))
      (writes (gymnast-surface-field add-behavior ':writes)))
    (assert-equal writes nil)))

(deftest mutant-remove-failure-mode-drops-fails
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (mutated (gymnast-mutate-remove-failure-mode ir 'add))
      (behaviors (gymnast-ir-nodes-of-kind mutated 'behavior))
      (add-behavior (car behaviors))
      (clauses (gymnast-ir-node-field add-behavior 'clauses))
      (fails (filter
          (lambda (c) (equal (gymnast-clause-head c) 'fails))
          clauses)))
    (assert-equal (length fails) 0)))

(deftest single-mutant-run-produces-result
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (mutant (gymnast-mutant 'weaken-add 'authorization
          "Remove preconditions from add behavior"
          (lambda (ir-val)
            (gymnast-mutate-weaken-precondition ir-val 'add))))
      (result (gymnast-run-mutant ir mutant)))
    (assert-true (gymnast-mutant-result-p result))
    (assert-equal
      (gymnast-mutant-result-field result 'mutant-id) 'weaken-add)
    (assert-equal
      (gymnast-mutant-result-field result 'class) 'authorization)))

(deftest campaign-runs-multiple-mutants
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (mutants (list
          (gymnast-mutant 'weaken-add 'authorization
            "Remove preconditions from add behavior"
            (lambda (ir-val)
              (gymnast-mutate-weaken-precondition ir-val 'add)))
          (gymnast-mutant 'skip-write 'persistence
            "Skip state writes in add behavior"
            (lambda (ir-val)
              (gymnast-mutate-skip-state-write ir-val 'add)))
          (gymnast-mutant 'remove-fails 'error-mapping
            "Remove failure modes from add behavior"
            (lambda (ir-val)
              (gymnast-mutate-remove-failure-mode ir-val 'add)))))
      (result (gymnast-run-campaign ir mutants)))
    (assert-true (gymnast-tagged-p 'campaign-result result))
    (assert-equal (gymnast-campaign-result-field result 'total) 3)
    (assert-equal
      (gymnast-campaign-result-field result 'schema)
      $gymnast-adequacy-schema)))

(deftest boundary-interleaving-generates-steps
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (scenario (gymnast-boundary-interleaving ir 5)))
    (assert-true scenario)
    (assert-true (gymnast-tagged-p 'interleaving-scenario scenario))
    (assert-equal
      (length (gymnast-assoc-value 'steps (cdr scenario))) 5)))

(deftest standard-fault-scenarios-defined
  (let ((faults (gymnast-standard-fault-scenarios)))
    (assert-equal (length faults) 4)
    (assert-true (gymnast-all
        (lambda (f) (gymnast-fault-scenario-p f))
        faults))))

(deftest campaign-reports-blind-spots-for-survivors
  (let* ((ir (gymnast-elaborate gymnast-test-spec))
      (mutants (list
          (gymnast-mutant 'identity-mutant 'identity
            "No-op mutant that should survive"
            (lambda (ir-val) ir-val))))
      (result (gymnast-run-campaign ir mutants))
      (survived (gymnast-campaign-result-field result 'survived)))
    (if (> survived 0)
      (assert-true (gymnast-campaign-result-field result 'blind-spots))
      (assert-equal survived 0))))

;;; Multi-target benchmark: all specs elaborate and plan to 8-node DAGs.

(load-file "examples/todo-rust.lisp")
(load-file "examples/twitter.lisp")
(load-file "examples/twitter-go.lisp")
(load-file "examples/twitter-java.lisp")
(load-file "examples/twitter-python.lisp")
(load-file "examples/twitter-rust.lisp")

(deftest todo-rust-elaborates-and-plans
  (let* ((ir (gymnast-elaborate todo-rust-spec))
      (plan (gymnast-plan ir)))
    (assert-equal (length (gymnast-plan-field plan 'nodes)) 8)))

(deftest twitter-ruby-elaborates-and-plans
  (let* ((ir (gymnast-elaborate twitter-spec))
      (plan (gymnast-plan ir)))
    (assert-equal (length (gymnast-plan-field plan 'nodes)) 8)))

(deftest twitter-go-elaborates-and-plans
  (let* ((ir (gymnast-elaborate twitter-go-spec))
      (plan (gymnast-plan ir)))
    (assert-equal (length (gymnast-plan-field plan 'nodes)) 8)))

(deftest twitter-java-elaborates-and-plans
  (let* ((ir (gymnast-elaborate twitter-java-spec))
      (plan (gymnast-plan ir)))
    (assert-equal (length (gymnast-plan-field plan 'nodes)) 8)))

(deftest twitter-python-elaborates-and-plans
  (let* ((ir (gymnast-elaborate twitter-python-spec))
      (plan (gymnast-plan ir)))
    (assert-equal (length (gymnast-plan-field plan 'nodes)) 8)))

(deftest twitter-rust-elaborates-and-plans
  (let* ((ir (gymnast-elaborate twitter-rust-spec))
      (plan (gymnast-plan ir)))
    (assert-equal (length (gymnast-plan-field plan 'nodes)) 8)))
