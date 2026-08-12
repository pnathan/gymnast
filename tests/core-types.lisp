(include "../src/gymnast.lisp")

(defspec core-types-test-spec
  :version "test"
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
    (requires (authenticated? user))
    (ensures (contains? post item)))
  (invariant no-duplicates :scope items :always (unique-ids? items))
  (synthesis test
    :target (lamedh :track "0.5")
    :model (small-code-model :class nano :temperature 0))
  (acceptance test
    :subject app
    (property round-trip
      :generate ((item valid-item))
      :execute (sequence (add item) (list))
      :must (contains? result item))))

;;; 1. Record predicates.

(deftest diagnostic-predicate-is-true-for-diagnostics
  (let ((d (gymnast-diagnostic 'error 'test "subj" "msg" nil)))
    (assert-true (gymnast-diagnostic-p d))))

(deftest diagnostic-predicate-is-false-for-other-records
  (let ((s (gymnast-make-surface 'behavior 'add '(:on add) nil 'fexpr)))
    (assert-false (gymnast-diagnostic-p s))))

(deftest surface-predicate-is-true-for-surfaces
  (let ((s (gymnast-make-surface 'behavior 'add '(:on add) nil 'fexpr)))
    (assert-true (gymnast-surface-p s))))

(deftest surface-predicate-is-false-for-other-records
  (let ((d (gymnast-diagnostic 'error 'test "subj" "msg" nil)))
    (assert-false (gymnast-surface-p d))))

(deftest invalid-surface-predicate-is-true-for-invalid-surfaces
  (let ((inv (gymnast-make-invalid-surface '(bad form) "malformed")))
    (assert-true (gymnast-invalid-surface-p inv))))

(deftest invalid-surface-predicate-is-false-for-other-records
  (let ((s (gymnast-make-surface 'behavior 'add '(:on add) nil 'fexpr)))
    (assert-false (gymnast-invalid-surface-p s))))

(deftest ir-node-predicate-is-true-for-ir-nodes
  (let ((n (gymnast-ir-node "test/b/x" 'behavior 'x
          '((:on add)) nil 'fexpr)))
    (assert-true (gymnast-ir-node-p n))))

(deftest ir-node-predicate-is-false-for-other-records
  (let ((d (gymnast-diagnostic 'error 'test "subj" "msg" nil)))
    (assert-false (gymnast-ir-node-p d))))

(deftest plan-node-predicate-is-true-for-plan-nodes
  (let ((p (gymnast-plan-node "t/p/x" 'structural 'test
          '("a") nil '(lamedh) '(none) '("a.lisp") nil nil nil)))
    (assert-true (gymnast-plan-node-p p))))

(deftest plan-node-predicate-is-false-for-other-records
  (let ((n (gymnast-ir-node "test/b/x" 'behavior 'x
          '((:on add)) nil 'fexpr)))
    (assert-false (gymnast-plan-node-p n))))

;;; 2. Record accessors.

(deftest diagnostic-accessors-return-constructor-args
  (let ((d (gymnast-diagnostic 'error 'test-code "subj" "a message"
          '(detail 1))))
    (assert-equal (gymnast-diagnostic-severity d) 'error)
    (assert-equal (gymnast-diagnostic-code d) 'test-code)
    (assert-equal (gymnast-diagnostic-subject d) "subj")
    (assert-equal (gymnast-diagnostic-message d) "a message")
    (assert-equal (gymnast-diagnostic-details d) '(detail 1))))

(deftest surface-accessors-return-constructor-args
  (let ((s (gymnast-make-surface 'behavior 'add '(:on add) '(child) 'fexpr)))
    (assert-equal (gymnast-surface-kind s) 'behavior)
    (assert-equal (gymnast-surface-name s) 'add)
    (assert-equal (gymnast-surface-operands s) '(:on add))
    (assert-equal (gymnast-surface-children s) '(child))
    (assert-equal (gymnast-surface-mechanism s) 'fexpr)))

(deftest invalid-surface-accessors-return-constructor-args
  (let ((inv (gymnast-make-invalid-surface '(bad form) "malformed")))
    (assert-equal (gymnast-invalid-surface-form inv) '(bad form))
    (assert-equal (gymnast-invalid-surface-message inv) "malformed")))

(deftest ir-node-accessors-return-constructor-args
  (let ((n (gymnast-ir-node "test/b/x" 'behavior 'x
          '((:on add)) '(clause-a) 'fexpr)))
    (assert-equal (gymnast-ir-node-id n) "test/b/x")
    (assert-equal (gymnast-ir-node-kind n) 'behavior)
    (assert-equal (gymnast-ir-node-name n) 'x)
    (assert-equal (gymnast-ir-node-clauses n) '(clause-a))
    (assert-equal (gymnast-ir-node-surface-mechanism n) 'fexpr)))

(deftest plan-node-accessors-return-constructor-args
  (let ((p (gymnast-plan-node "t/p/x" 'structural 'test-v1
          '("a") '("dep") '(lamedh) '(none) '("a.lisp") nil nil nil)))
    (assert-equal (gymnast-plan-node-id p) "t/p/x")
    (assert-equal (gymnast-plan-node-class p) 'structural)
    (assert-equal (gymnast-plan-node-recipe p) 'test-v1)
    (assert-equal (gymnast-plan-node-target p) '(lamedh))
    (assert-equal (gymnast-plan-node-model p) '(none))))

;;; 3. Dynamic field access via record-ref.

(deftest diagnostic-field-matches-accessor
  (let ((d (gymnast-diagnostic 'warning 'code-x "subj" "msg" nil)))
    (assert-equal (gymnast-diagnostic-field d 'severity)
      (gymnast-diagnostic-severity d))
    (assert-equal (gymnast-diagnostic-field d 'code) 'code-x)))

(deftest ir-node-field-matches-accessor
  (let ((n (gymnast-ir-node "test/b/x" 'behavior 'x
          '((:on add)) nil 'fexpr)))
    (assert-equal (gymnast-ir-node-field n 'id) (gymnast-ir-node-id n))
    (assert-equal (gymnast-ir-node-field n 'kind) 'behavior)))

(deftest plan-node-field-matches-accessor
  (let ((p (gymnast-plan-node "t/p/x" 'structural 'test
          '("a") nil '(lamedh) '(none) '("a.lisp") nil nil nil)))
    (assert-equal (gymnast-plan-node-field p 'recipe) 'test)
    (assert-equal (gymnast-plan-node-field p 'may-write) '("a.lisp"))))

(deftest record-ref-works-directly-on-diagnostics
  (let ((d (gymnast-diagnostic 'error 'code-y "subj" "msg" nil)))
    (assert-equal (record-ref d 'subject) "subj")
    (assert-equal (record-ref d 'message) "msg")))

;;; 4. Structural equality.

(deftest equal-diagnostics-with-same-fields-are-equal
  (let ((a (gymnast-diagnostic 'error 'x "s" "m" nil))
      (b (gymnast-diagnostic 'error 'x "s" "m" nil)))
    (assert-equal a b)))

(deftest unequal-diagnostics-with-different-fields-are-not-equal
  (let ((a (gymnast-diagnostic 'error 'x "s" "m" nil))
      (b (gymnast-diagnostic 'warning 'x "s" "m" nil)))
    (assert-false (equal a b))))

(deftest equal-surfaces-with-same-fields-are-equal
  (let ((a (gymnast-make-surface 'behavior 'add '(:on add) nil 'fexpr))
      (b (gymnast-make-surface 'behavior 'add '(:on add) nil 'fexpr)))
    (assert-equal a b)))

(deftest equal-plan-nodes-with-same-fields-are-equal
  (let ((a (gymnast-plan-node "t/p/s" 'structural 'test
          '("b" "a") nil '(ruby) '(none) '("out.lisp") nil nil nil))
      (b (gymnast-plan-node "t/p/s" 'structural 'test
          '("b" "a") nil '(ruby) '(none) '("out.lisp") nil nil nil)))
    (assert-equal a b)))

;;; 5. Functional update via record-with.

(deftest record-with-updates-a-single-field
  (let* ((d (gymnast-diagnostic 'error 'x "s" "m" nil))
      (updated (record-with d 'severity 'warning)))
    (assert-equal (gymnast-diagnostic-severity updated) 'warning)
    (assert-equal (gymnast-diagnostic-code updated) 'x)))

(deftest record-with-leaves-original-record-unchanged
  (let* ((d (gymnast-diagnostic 'error 'x "s" "m" nil))
      (updated (record-with d 'severity 'warning)))
    (assert-equal (gymnast-diagnostic-severity d) 'error)
    (assert-false (equal d updated))))

(deftest record-with-on-surface-updates-kind
  (let* ((s (gymnast-make-surface 'behavior 'add '(:on add) nil 'fexpr))
      (updated (record-with s 'kind 'invariant)))
    (assert-equal (gymnast-surface-kind updated) 'invariant)
    (assert-equal (gymnast-surface-name updated) 'add)))

;;; 6. Wrapper constructors.

(deftest diagnostic-wrapper-constructor-produces-valid-record
  (let ((d (gymnast-diagnostic 'error 'w "subj" "msg" nil)))
    (assert-true (gymnast-diagnostic-p d))
    (assert-true (gymnast-error-diagnostic-p d))))

(deftest surface-wrapper-constructor-produces-valid-record
  (let ((s (gymnast-make-surface 'type 'Foo '(:opaque Text) nil 'vau)))
    (assert-true (gymnast-surface-p s))
    (assert-equal (gymnast-surface-mechanism s) 'vau)))

(deftest ir-node-wrapper-constructor-produces-valid-record
  (let ((n (gymnast-ir-node "test/t/y" 'type 'y nil nil 'vau)))
    (assert-true (gymnast-ir-node-p n))))

(deftest plan-node-wrapper-constructor-produces-valid-record
  (let ((p (gymnast-plan-node "t/p/y" 'generative 'kernel-v1
          '("x") nil '(lamedh) '(small-code-model) '("y.lisp") nil nil nil)))
    (assert-true (gymnast-plan-node-p p))))

;;; 7. IR node canonicalization.

(deftest ir-node-constructor-sorts-fields-alphabetically-by-key
  (let* ((n (gymnast-ir-node "test/b/x" 'behavior 'x
          '((:writes (items)) (:on add) (:reads (items)))
          nil 'fexpr))
      (fields (gymnast-ir-node-fields n))
      (keys (mapcar (lambda (f) (gymnast-symbol-string (car f))) fields)))
    (assert-equal keys (sort (append keys nil) #'string-lessp))
    (assert-equal (car (car fields)) ':on)))

(deftest ir-node-constructor-preserves-all-fields
  (let* ((n (gymnast-ir-node "test/b/x" 'behavior 'x
          '((:writes (items)) (:on add) (:reads (items)))
          nil 'fexpr))
      (fields (gymnast-ir-node-fields n)))
    (assert-equal (length fields) 3)
    (assert-equal (gymnast-assoc-value ':writes fields) '(items))))

(deftest empty-fields-canonicalize-to-empty
  (let ((n (gymnast-ir-node "test/t/z" 'type 'z nil nil 'vau)))
    (assert-equal (gymnast-ir-node-fields n) nil)))

;;; 8. Plan node sorting and fingerprinting.

(deftest plan-node-sorts-inputs-alphabetically
  (let ((p (gymnast-plan-node "t/p/x" 'structural 'test
          '("z" "a" "m") nil '(ruby) '(none) '("z.lisp" "a.lisp")
          nil nil nil)))
    (assert-equal (gymnast-plan-node-inputs p) '("a" "m" "z"))))

(deftest plan-node-sorts-depends-on-alphabetically
  (let ((p (gymnast-plan-node "t/p/x" 'structural 'test
          nil '("z-dep" "a-dep") '(ruby) '(none) '("out.lisp")
          nil nil nil)))
    (assert-equal (gymnast-plan-node-depends-on p) '("a-dep" "z-dep"))))

(deftest plan-node-sorts-may-write-alphabetically
  (let ((p (gymnast-plan-node "t/p/x" 'structural 'test
          '("z" "a" "m") nil '(ruby) '(none) '("z.lisp" "a.lisp")
          nil nil nil)))
    (assert-equal (gymnast-plan-node-may-write p) '("a.lisp" "z.lisp"))))

(deftest plan-node-computes-a-string-fingerprint
  (let ((p (gymnast-plan-node "t/p/x" 'structural 'test
          '("a") nil '(ruby) '(none) '("a.lisp") nil nil nil)))
    (assert-true (stringp (gymnast-plan-node-fingerprint p)))
    (assert-true (> (length (gymnast-plan-node-fingerprint p)) 0))))

;;; 9. Plan node fingerprint stability.

(deftest plan-node-fingerprint-is-stable-across-constructions
  (let ((a (gymnast-plan-node "t/p/s" 'structural 'test
          '("b" "a") nil '(ruby) '(none) '("out.lisp") nil nil nil))
      (b (gymnast-plan-node "t/p/s" 'structural 'test
          '("b" "a") nil '(ruby) '(none) '("out.lisp") nil nil nil)))
    (assert-equal (gymnast-plan-node-fingerprint a)
      (gymnast-plan-node-fingerprint b))))

(deftest plan-node-fingerprint-is-order-independent-for-unsorted-inputs
  (let ((a (gymnast-plan-node "t/p/s" 'structural 'test
          '("a" "b") nil '(ruby) '(none) '("out.lisp") nil nil nil))
      (b (gymnast-plan-node "t/p/s" 'structural 'test
          '("b" "a") nil '(ruby) '(none) '("out.lisp") nil nil nil)))
    (assert-equal (gymnast-plan-node-fingerprint a)
      (gymnast-plan-node-fingerprint b))))

(deftest plan-node-fingerprint-differs-for-different-content
  (let ((a (gymnast-plan-node "t/p/s" 'structural 'test
          '("a") nil '(ruby) '(none) '("out.lisp") nil nil nil))
      (b (gymnast-plan-node "t/p/s" 'structural 'test
          '("a" "c") nil '(ruby) '(none) '("out.lisp") nil nil nil)))
    (assert-false (equal
        (gymnast-plan-node-fingerprint a)
        (gymnast-plan-node-fingerprint b)))))

;;; 10. Records are not conses.

(deftest diagnostic-is-an-atom-not-a-cons
  (let ((d (gymnast-diagnostic 'error 'x "s" "m" nil)))
    (assert-false (consp d))
    (assert-true (atom d))))

(deftest plan-node-is-an-atom-not-a-cons
  (let ((p (gymnast-plan-node "t/p/x" 'structural 'test
          '("a") nil '(ruby) '(none) '("a.lisp") nil nil nil)))
    (assert-false (consp p))
    (assert-true (atom p))))

(deftest ir-node-is-an-atom-not-a-cons
  (let ((n (gymnast-ir-node "test/t/z" 'type 'z nil nil 'vau)))
    (assert-false (consp n))
    (assert-true (atom n))))

;;; 11. Integration with the existing pipeline.

(deftest elaboration-produces-ir-nodes-that-are-records
  (let* ((ir (gymnast-elaborate core-types-test-spec))
      (behaviors (gymnast-ir-nodes-of-kind ir 'behavior)))
    (assert-equal (length behaviors) 1)
    (assert-true (gymnast-ir-node-p (car behaviors)))))

(deftest planning-produces-plan-nodes-that-are-records
  (let* ((ir (gymnast-elaborate core-types-test-spec))
      (plan (gymnast-plan ir))
      (nodes (gymnast-plan-field plan 'nodes)))
    (assert-equal (length nodes) 8)
    (assert-true (gymnast-all #'gymnast-plan-node-p nodes))))

(deftest prompt-compilation-covers-every-plan-node
  (let* ((ir (gymnast-elaborate core-types-test-spec))
      (plan (gymnast-plan ir))
      (prompts (gymnast-compile-prompts ir plan)))
    (assert-equal (length prompts) (length (gymnast-plan-field plan 'nodes)))))

(deftest elaboration-has-no-error-diagnostics
  (let ((ir (gymnast-elaborate core-types-test-spec)))
    (assert-false (gymnast-has-errors-p (gymnast-ir-field ir 'diagnostics)))))

(deftest plan-node-lookup-by-id-returns-a-record
  (let* ((ir (gymnast-elaborate core-types-test-spec))
      (plan (gymnast-plan ir))
      (node (gymnast-find-plan-node plan
          (gymnast-plan-id ir "design-contracts"))))
    (assert-true (gymnast-plan-node-p node))
    (assert-true (gymnast-strings-canonical-p
        (gymnast-plan-node-field node 'inputs)))))
