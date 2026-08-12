(include "../src/gymnast.lisp")

(defspec transition-test-spec
  :version "test"
  (actor user :kind person)
  (type UserId :opaque Text)
  (type Item :record ((id UserId) (title Text)))
  (component app :responsibility "Test" :provides (api))
  (interface api
    (command add :actor user :input Item :output Item :errors (forbidden)))
  (state items :of (List Item) :owner app :durability durable)
  (flow access :from user :to api :kind command :grant (authenticated))
  (behavior add
    :on (api/add user item)
    :reads (items) :writes (items) :atomic items
    (requires (authenticated? user))
    (ensures (contains? post item)))
  (invariant no-duplicates :scope items :always (unique-ids? items))
  (synthesis test
    :target (lamedh :track "0.5")
    :model (small-code-model :class nano :temperature 0))
  (acceptance test :subject app
    (property round-trip
      :generate ((item valid-item))
      :execute (sequence (add item) (list))
      :must (contains? result item))))

(deftest transition-is-a-record
  (let* ((ir (gymnast-elaborate transition-test-spec))
      (transitions (gymnast-extract-transitions ir))
      (tr (car transitions)))
    (assert-true (gymnast-transition-p tr))
    (assert-equal (gymnast-transition-field tr 'operation) "api/add")
    (assert-equal (gymnast-transition-operation tr) "api/add")))

(deftest trace-step-is-a-record
  (let* ((ir (gymnast-elaborate transition-test-spec))
      (trace (gymnast-execute-trace ir
          (list (list "api/add" "user1" "item1")))))
    (let ((steps (gymnast-trace-field trace 'steps)))
      (assert-true (> (length steps) 0))
      (assert-true (gymnast-trace-step-p (car steps))))))

(deftest transition-record-accessors
  (let* ((ir (gymnast-elaborate transition-test-spec))
      (transitions (gymnast-extract-transitions ir))
      (tr (car transitions)))
    (assert-true (stringp (gymnast-transition-id tr)))
    (assert-true (not (null (gymnast-transition-preconditions tr))))))

(deftest trace-step-record-ref
  (let* ((ir (gymnast-elaborate transition-test-spec))
      (trace (gymnast-execute-trace ir
          (list (list "api/add" "user1" "item1")))))
    (let ((step (car (gymnast-trace-field trace 'steps))))
      (assert-equal (record-ref step 'actor) "user1")
      (assert-equal (record-ref step 'input) "item1"))))
