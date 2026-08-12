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
