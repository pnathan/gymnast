(include "../src/gymnast.lisp")

(deftest cache-entry-is-a-record
  (let ((entry (gymnast-cache-entry "key1" "node/plan/x"
          '(candidate) '(evidence) 'now)))
    (assert-true (gymnast-cache-entry-p entry))
    (assert-equal (gymnast-cache-entry-key entry) "key1")
    (assert-equal (gymnast-cache-entry-node-id entry) "node/plan/x")
    (assert-equal (gymnast-cache-entry-schema entry) $gymnast-cache-schema)))

(deftest cache-entry-field-uses-record-ref
  (let ((entry (gymnast-cache-entry "k" "n" 'c 'e 't)))
    (assert-equal (gymnast-cache-entry-field entry 'key) "k")
    (assert-equal (gymnast-cache-entry-field entry 'candidate) 'c)))

(deftest cache-store-and-lookup-with-records
  (gymnast-cache-clear)
  (let ((entry (gymnast-cache-entry "test-key" "n" 'c 'e 'now)))
    (gymnast-cache-store "test-key" entry)
    (let ((found (gymnast-cache-lookup "test-key")))
      (assert-true (gymnast-cache-entry-p found))
      (assert-equal found entry)))
  (gymnast-cache-clear))

(deftest mutant-is-a-record
  (let ((m (gymnast-mutant 'mut1 'weaken-precondition "test" #'identity)))
    (assert-true (gymnast-mutant-p m))
    (assert-equal (gymnast-mutant-id m) 'mut1)
    (assert-equal (gymnast-mutant-class m) 'weaken-precondition)
    (assert-equal (gymnast-mutant-critical m) t)))

(deftest mutant-field-uses-record-ref
  (let ((m (gymnast-mutant 'mut2 'remove-invariant "desc" #'identity)))
    (assert-equal (gymnast-mutant-field m 'id) 'mut2)
    (assert-equal (gymnast-mutant-field m 'description) "desc")))

(deftest fault-scenario-is-a-record
  (let ((fs (gymnast-make-fault-scenario 'restart 'restart 'write)))
    (assert-true (gymnast-fault-scenario-p fs))
    (assert-equal (gymnast-fault-scenario-name fs) 'restart)
    (assert-equal (gymnast-fault-scenario-expected fs) 'detected)))

(deftest standard-fault-scenarios-are-records
  (let ((scenarios (gymnast-standard-fault-scenarios)))
    (assert-equal (length scenarios) 4)
    (assert-true (gymnast-fault-scenario-p (car scenarios)))))
