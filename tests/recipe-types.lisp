(include "../src/gymnast.lisp")

(deftest ruby-target-is-a-record
  (assert-true (gymnast-ruby-target-p $gymnast-ruby-target))
  (assert-equal (gymnast-ruby-target-name $gymnast-ruby-target) 'ruby))

(deftest protocol-dispatch-produces-ruby-header
  (let ((header (gymnast-emit-comment-header $gymnast-ruby-target "Test")))
    (assert-true (stringp header))
    (assert-true (starts-with-p header "# frozen_string_literal: true"))))

(deftest protocol-dispatch-matches-direct-call
  (let ((via-protocol (gymnast-emit-comment-header $gymnast-ruby-target "X"))
      (via-wrapper (gymnast-ruby-comment-header "X")))
    (assert-equal via-protocol via-wrapper)))

(deftest recipe-executor-uses-protocol-dispatch
  (let* ((surface
        (module recipe-test
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
      (succeeded (filter
          (lambda (r)
            (equal (gymnast-execution-result-field r 'status) 'succeeded))
          results)))
    (assert-true (> (length succeeded) 0))))
