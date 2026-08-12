(include "../src/gymnast.lisp")

(deftest artifact-is-a-record
  (let ((a (make-gymnast-artifact "path/file.lisp" "node/plan/x"
          "fnv1a64:12345" 100)))
    (assert-true (gymnast-artifact-p a))
    (assert-equal (gymnast-artifact-path a) "path/file.lisp")
    (assert-equal (gymnast-artifact-node-id a) "node/plan/x")
    (assert-equal (gymnast-artifact-size a) 100)))

(deftest artifact-field-uses-record-ref
  (let ((a (make-gymnast-artifact "p" "n" "d" 50)))
    (assert-equal (gymnast-artifact-field a 'path) "p")
    (assert-equal (gymnast-artifact-field a 'digest) "d")))

(deftest traceability-entry-is-a-record
  (let ((te (make-gymnast-traceability-entry "mod/behavior/add"
          'behavior '("mod/plan/handlers") t nil)))
    (assert-true (gymnast-traceability-entry-p te))
    (assert-equal (gymnast-traceability-entry-semantic-id te)
      "mod/behavior/add")
    (assert-equal (gymnast-traceability-entry-kind te) 'behavior)
    (assert-true (gymnast-traceability-entry-has-implementation te))))

(deftest traceability-entry-field-uses-record-ref
  (let ((te (make-gymnast-traceability-entry "id" 'type '() nil nil)))
    (assert-equal (gymnast-traceability-entry-field te 'semantic-id) "id")
    (assert-equal (gymnast-traceability-entry-field te 'has-implementation) nil)))

(deftest assembly-integration-with-records
  (let* ((surface
        (module assembly-test
          (actor user :kind person)
          (type UserId :opaque Text)
          (type Item :record ((id UserId) (title Text)))
          (component app :responsibility "Test" :provides (api))
          (interface api
            (command add :actor user :input Item :output Item
              :errors (forbidden)))
          (state items :of (List Item) :owner app :durability durable)
          (flow access :from user :to api :kind command
            :grant (authenticated))
          (behavior add
            :on (api/add user item)
            :reads (items) :writes (items) :atomic items
            (requires (authenticated? user))
            (ensures (contains? post item)))
          (invariant no-dups :scope items :always (unique-ids? items))
          (synthesis s :target (lamedh :track "0.5")
            :model (small-code-model :class nano :temperature 0))
          (acceptance t :subject app
            (property p :generate ((x g))
              :execute (sequence (add x) (list))
              :must (ok? result)))))
      (ir (gymnast-elaborate surface))
      (plan (gymnast-plan ir))
      (results (gymnast-execute-deterministic ir plan))
      (verification (gymnast-compile-verification ir))
      (bundle (gymnast-assemble-bundle ir plan results verification)))
    (assert-true (not (null bundle)))
    (let ((artifacts (gymnast-bundle-field bundle 'artifacts)))
      (assert-true (> (length artifacts) 0))
      (assert-true (gymnast-artifact-p (car artifacts))))
    (let ((traceability (gymnast-bundle-field bundle 'traceability)))
      (assert-true (> (length traceability) 0))
      (assert-true (gymnast-traceability-entry-p (car traceability))))))
