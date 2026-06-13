use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use url::Url;

/// Запись в очереди обхода
#[derive(Debug, Clone)]
pub struct UrlEntry {
    pub url: Url,
    pub depth: u32,
    pub priority: i32,
    pub parent: Option<Url>,
}

impl PartialEq for UrlEntry {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}
impl Eq for UrlEntry {}

impl PartialOrd for UrlEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UrlEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap — max-heap; нам нужен наибольший priority наверху
        self.priority.cmp(&other.priority)
    }
}

/// Потокобезопасная приоритетная очередь URL.
///
/// Гарантирует:
/// - уникальность URL в очереди (не добавляет дубликаты)
/// - порядок извлечения по убыванию приоритета
/// - пробуждение ожидающих воркеров при добавлении новых URL
pub struct UrlQueue {
    heap:     Mutex<BinaryHeap<UrlEntry>>,
    in_queue: Mutex<HashSet<String>>,
    notify:   Notify,
}

impl UrlQueue {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            heap:     Mutex::new(BinaryHeap::new()),
            in_queue: Mutex::new(HashSet::new()),
            notify:   Notify::new(),
        })
    }

    /// Добавить URL в очередь. Возвращает false если URL уже есть в очереди.
    pub async fn push(&self, entry: UrlEntry) -> bool {
        let key = entry.url.to_string();
        let mut in_queue = self.in_queue.lock().await;

        if in_queue.contains(&key) {
            return false;
        }
        in_queue.insert(key);
        drop(in_queue);

        self.heap.lock().await.push(entry);
        // Разбудить одного ожидающего воркера
        self.notify.notify_one();
        true
    }

    /// Извлечь URL с наибольшим приоритетом (неблокирующий)
    pub async fn pop(&self) -> Option<UrlEntry> {
        let mut heap = self.heap.lock().await;
        if let Some(entry) = heap.pop() {
            let mut in_queue = self.in_queue.lock().await;
            in_queue.remove(&entry.url.to_string());
            Some(entry)
        } else {
            None
        }
    }

    /// Подождать появления URL, затем извлечь.
    /// Следует использовать с `tokio::select!` для отмены.
    pub async fn pop_or_wait(&self) -> UrlEntry {
        loop {
            if let Some(entry) = self.pop().await {
                return entry;
            }
            self.notify.notified().await;
        }
    }

    pub async fn len(&self) -> usize {
        self.heap.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.heap.lock().await.is_empty()
    }

    /// Снимок верхних N элементов для отображения в TUI (не извлекает)
    pub async fn peek_top(&self, n: usize) -> Vec<(String, i32)> {
        let heap = self.heap.lock().await;
        // BinaryHeap не даёт итерацию в порядке приоритета без копии,
        // но для TUI нам достаточно произвольного среза
        heap.iter()
            .take(n)
            .map(|e| (e.url.to_string(), e.priority))
            .collect()
    }

    /// Сигнализировать всем ожидающим воркерам о завершении работы
    pub fn notify_all_waiters(&self) {
        self.notify.notify_waiters();
    }
}
