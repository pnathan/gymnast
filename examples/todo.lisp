;;; A vertical-slice application specification.  This is intentionally more
;;; explicit than the eventual product surface; the kernel is under review.

(defspec todo-spec
  :version "0.1"
  :owner product
  :exports (UserId ListId TaskId TodoList Task todo-service)

  (use-profile oddities/profiles/todo-standard "1.0"
    :sharing-limit 256
    :identity-provider google)

  (application todo
    :modules (todo-spec)
    :default-acceptance production)

  (actor user
    :kind person
    :identity (google-openid issuer subject))

  (type UserId :opaque Text)
  (type ListId :opaque Text)
  (type TaskId :opaque Text)
  (type Version :opaque Integer)
  (type Role :enum (owner editor viewer))
  (type TaskStatus :enum (open completed))
  (type TaskState :enum (active trashed))
  (type Due
    :variant ((date-only LocalDate)
      (at ZonedDateTime)))

  (type TodoList
    :record ((id ListId)
      (title (Text :min 1 :max 200))
      (owner UserId)
      (version Version)))

  (type Task
    :record ((id TaskId)
      (list ListId)
      (title (Text :min 1 :max 500))
      (notes (Text :max 20000))
      (status TaskStatus)
      (state TaskState)
      (due (Optional Due))
      (assignee (Optional UserId))
      (version Version)))

  (component todo-app
    :responsibility "Manage persistent, shareable Todo lists"
    :provides (todo-service)
    :uses (google-identity durable-store clock id-source))

  (interface todo-service
    (command create-task
      :actor user
      :input (record (list ListId) (title Text) (due (Optional Due)))
      :output Task
      :errors (unauthenticated forbidden not-found conflict))
    (query query-tasks
      :actor user
      :input (record (list ListId) (cursor (Optional Cursor)))
      :output (Page Task)
      :errors (unauthenticated not-found invalid-cursor))
    (command invite
      :actor user
      :input (record (list ListId) (principal UserId) (role Role))
      :output Membership
      :errors (unauthenticated forbidden not-found sharing-limit)))

  (state todo-state
    :of (aggregate TodoList Task Membership Invitation)
    :owner todo-app
    :durability durable
    :initial empty
    :aggregate (per-list ListId)
    :versioned optimistic
    :consistency serial-per-list)

  (flow authenticated-user-to-service
    :from user
    :to todo-service
    :kind command
    :grant (authenticated-session)
    :deny (raw-google-token))

  (behavior create-task
    :on (todo-service/create-task user request)
    :reads (memberships todo-lists)
    :writes (tasks)
    :atomic (list request/list)
    :idempotency command-key
    (requires (authenticated? user))
    (requires (may-edit-list? pre user request/list))
    (ensures (= post (insert-task pre request result)))
    (returns result)
    (fails forbidden
      :when (not (may-edit-list? pre user request/list))
      :preserves all-state)
    (emits task-created :exactly-once-logically))

  (behavior invite-user
    :on (todo-service/invite user request)
    :reads (memberships invitations)
    :writes (invitations)
    :atomic (list-membership request/list)
    :idempotency command-key
    (requires (owner? pre user request/list))
    (requires (< (other-principal-count pre request/list) 256))
    (ensures (= post (invite-principal pre request)))
    (fails sharing-limit
      :when (= (other-principal-count pre request/list) 256)
      :preserves all-state))

  (invariant owner-isolation
    :scope todo-state
    :always (no-observation-without-active-membership))

  (invariant sharing-limit
    :scope todo-state
    :always (forall ((list TodoList))
      (<= (other-principal-count list) 256)))

  (constraint collaborative-capacity
    :class workload
    :scope todo-service
    :under (workload
      :virtual-users 500
      :duration (minutes 30)
      :read-p95 (milliseconds 300)
      :write-p95 (milliseconds 500))
    :must (and (= lost-updates 0)
      (= invariant-violations 0)))

  (synthesis prototype
    :target (lamedh :track "0.5")
    :platform gymnast-reference-platform-v1
    :model (small-code-model
      :class nano
      :temperature 0
      :max-attempts 3)
    :attempts 3
    :must-not (invent-product-semantics add-unpinned-dependencies))

  (acceptance production
    :subject todo-app
    (property create-then-read
      :generate ((actor authenticated-editor) (task valid-task))
      :execute (sequence (create-task actor task)
        (query-tasks actor task/list))
      :must (contains-equivalent? result task))
    (property viewer-cannot-mutate
      :generate ((actor authenticated-viewer) (task valid-task))
      :execute (create-task actor task)
      :must (fails-with forbidden))
    (scenario sharing-boundary
      (given owner (authenticated-owner))
      (when (invite-distinct owner 256))
      (then succeeds)
      (when (invite-distinct owner 257))
      (then (fails-with sharing-limit)))
    (concurrency boundary-race
      :actors 500
      :schedule adversarial
      :must (<= active-and-pending-other-principals 256))
    (fault durable-restart
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
