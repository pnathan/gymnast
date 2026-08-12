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
