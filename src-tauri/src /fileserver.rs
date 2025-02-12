use log::{info, error};
use std::sync::Arc;
use std::{
    fs::{self, File},
    path::PathBuf,
};
use lazy_static::lazy_static;
use warp::Filter;
use serde_json::Value;
use warp::reply::Html;
use std::collections::{HashSet, HashMap};
use tokio::sync::{Mutex, RwLock};
use serde::{Serialize, Deserialize};
use reqwest::Client;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use crate::printpdf::EXECUTION_DATA;


#[derive(Serialize, Clone, Debug)]
struct Metric {
    task_id: String,
    method_name: String,
    duration: u128,
}

#[derive(Serialize)]
struct MetricsResponse {
    total_task_ids: usize,
    metrics: Vec<Metric>,
}

pub async fn start_server(temp_dir: Arc<Mutex<PathBuf>>) {
    let temp_dir_path = temp_dir.lock().await.clone();
    info!("启动 HTTP 服务器, 托管目录: {}", temp_dir_path.display());

    let pdf_route = warp::path!("pdf" / String)
        .and(with_temp_dir(temp_dir.clone()))
        .and_then(handle_pdf_request);

    let metrics_route = warp::path!("metrics")
        .and(warp::get())
        .and_then(handle_metrics_request);

    let start_task_route = warp::path!("start_task")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(handle_start_task_request);

    let frontend_route = warp::path!("metrics" / "page")
        .and(warp::get())
        .and_then(handle_frontend_request);

    let routes = pdf_route.or(metrics_route).or(start_task_route).or(frontend_route);

    warp::serve(routes)
        .run(([127,0,0,1],10819))
        .await;
}

fn with_temp_dir(
    temp_dir: Arc<Mutex<PathBuf>>,
) -> impl Filter<Extract=(Arc<Mutex<PathBuf>>,),Error=std::convert::Infallible>+Clone {
    warp::any().map(move|| temp_dir.clone())
}

async fn handle_pdf_request(filename:String,temp_dir:Arc<Mutex<PathBuf>>)->Result<impl warp::Reply,warp::Rejection>{
    let file_path=temp_dir.lock().await.join(&filename);
    info!("Serving PDF at: {}", file_path.display());

    match fs::read(&file_path) {
        Ok(pdf_data)=>
            Ok(warp::http::Response::builder()
               .header("Content-Type","application/pdf")
               .body(pdf_data)),
        Err(_) =>
            Ok(warp::http::Response::builder()
               .status(404)
               .body("PDF file not found".into()))
    }
}

async fn handle_metrics_request()->Result<impl warp::Reply,warp::Rejection>{
    info!("开始处理metrics请求");
    let data=EXECUTION_DATA.read().await;
    info!("读取到的数据: {:?}", *data);

    // 只展示已结束任务的记录
    let ended_data:Vec<_>=data.iter().filter(|exec| exec.end_time.is_some()).collect();
    let metrics:Vec<Metric>=ended_data.into_iter().map(|exec| {
        let duration=exec.duration.unwrap_or(0);
        Metric {
            task_id:exec.task_id.clone(),
            method_name:exec.method_name.clone(),
            duration,
        }
    }).collect();

    let unique_task_ids:HashSet<_>=metrics.iter().map(|m|&m.task_id).collect();
    let total_task_ids=unique_task_ids.len();

    let response=MetricsResponse{total_task_ids,metrics};
    info!("成功返回metrics数据");
    Ok(warp::reply::json(&response))
}

#[derive(Serialize,Deserialize,Debug)]
struct StartTaskRequest{
    method_name:String,
    printdata:String,
    options:Value,
}

async fn handle_start_task_request(req:StartTaskRequest)->Result<impl warp::Reply,warp::Rejection>{
    let task_id=Uuid::new_v4().to_string();
    let task_id_for_spawn = task_id.clone();
    let method_name=req.method_name.clone();
    let printdata=req.printdata.clone();
    let options=req.options.clone();

    tokio::spawn(async move {
        match crate::printpdf::start_print_pdf(task_id_for_spawn.clone(),printdata,options).await {
            Ok(file_path)=>
            info!("任务{}完成,PDF路径:{:?}",task_id_for_spawn, file_path),
            Err(err)=>error!("任务{}失败:{}",task_id_for_spawn,err),
        }
    });

    let response=serde_json::json!({
        "task_id":task_id,
        "method_name":method_name,
        "status":"started"
    });
    Ok(warp::reply::json(&response))
}

async fn handle_frontend_request()->Result<impl warp::Reply,warp::Rejection>{
    // 前端HTML与之前相同
    let html=r#"
    <!DOCTYPE html>
    <html>
    <head>
        <meta charset="UTF-8">
        <title>线程执行监控(已完成任务)</title>
        <script src="https://cdn.jsdelivr.net/npm/echarts/dist/echarts.min.js"></script>
<style>
    body {
        font-family: Arial, sans-serif;
        background-color: #f4f4f9;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
    }

    h2 {
        color: #333;
        margin-top: 20px;
        margin-bottom: 10px;
    }

    #chart-container {
        width: 100%;
        max-width: 1200px;
        margin-bottom: 40px;
        position: relative;
        background-color: #fff;
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
        border-radius: 8px;
        padding: 20px;
        box-sizing: border-box;
    }

    #chart {
        width: 100%;
        height: 600px;
    }

    @media (max-width: 768px) {
        #chart {
            height: 400px;
        }
        h2 {
            font-size: 1.5em;
        }
    }

    #loading {
        position: absolute;
        top: 50%;
        left: 50%;
        transform: translate(-50%, -50%);
        font-size: 1.2em;
        color: #555;
    }

    #error-message {
        color: red;
        font-weight: bold;
        margin-top: 20px;
    }

    #pagination {
        display: flex;
        justify-content: center;
        gap: 10px;
        margin-top: 20px;
    }

    #pagination button {
        padding: 8px 12px;
        border: none;
        border-radius: 4px;
        background-color: #5470C6;
        color: #fff;
        cursor: pointer;
        transition: background-color 0.3s;
    }

    #pagination button:hover {
        background-color: #425aa0;
    }

    #pagination button.active {
        background-color: #91CC75;
    }
</style>

    </head>
    <body>
        <h2>线程执行监控(仅显示已完成任务的最终耗时)</h2>
        <div id="chart-container">
            <div id="chart"></div>
            <div id="loading">加载中...</div>
        </div>
        <div id="error-message"></div>
        <div id="pagination"></div>
        <script>
            let currentPage = 1;
            const pageSize = 20; 

            async function fetchData() {
                try {
                    const response = await fetch('/metrics');
                    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
                    const data = await response.json();
                    console.log('Fetched data:', data);
                    return data;
                } catch (error) {
                    console.error('Error fetching data:', error);
                    document.getElementById('error-message').textContent = '无法获取数据，请稍后再试。';
                    return { total_task_ids: 0, metrics: [] };
                }
            }

            function getColor(index) {
                const colors = ['#5470C6','#91CC75','#EE6666','#FAC858','#73C0DE','#3BA272','#FC8452','#9A60B4','#EA7CCC'];
                return colors[index % colors.length];
            }

            function addPaginationControls(total, pageSize) {
                const paginationContainer = document.getElementById('pagination');
                paginationContainer.innerHTML = '';
                const totalPages = Math.ceil(total / pageSize);
                if (totalPages <= 1) return;
                for (let i = 1; i <= totalPages; i++) {
                    const button = document.createElement('button');
                    button.textContent = i;
                    if (i === currentPage) button.classList.add('active');
                    button.onclick = () => {
                        currentPage = i;
                        updateChartPaginated();
                        highlightActivePage();
                    };
                    paginationContainer.appendChild(button);
                }
            }

            function highlightActivePage() {
                const buttons = document.querySelectorAll('#pagination button');
                buttons.forEach(button => {
                    button.classList.remove('active');
                    if (parseInt(button.textContent) === currentPage) {
                        button.classList.add('active');
                    }
                });
            }

            function renderChartPaginated(data, page, pageSize) {
                console.log(`Rendering page ${page} with page size ${pageSize}`);
                console.log(`Total unique task_ids: ${data.total_task_ids}`);
                console.log(`Total metrics length: ${data.metrics.length}`);

                const taskIdMap = {};
                data.metrics.forEach(item => {
                    if (!taskIdMap[item.task_id]) {
                        taskIdMap[item.task_id] = { methods: {} };
                    }
                    taskIdMap[item.task_id].methods[item.method_name] = item.duration || 0;
                });

                const taskIds = Object.keys(taskIdMap);
                const totalTasks = taskIds.length;
                const totalPages = Math.ceil(totalTasks / pageSize);
                if (page > totalPages && totalPages > 0) page = totalPages;

                const paginatedTaskIds = taskIds.slice((page - 1) * pageSize, page * pageSize);

                const methodNamesSet = new Set();
                paginatedTaskIds.forEach(task_id => {
                    Object.keys(taskIdMap[task_id].methods).forEach(method => {
                        methodNamesSet.add(method);
                    });
                });
                const methodNames = [...methodNamesSet].sort();

                const series = methodNames.map((method, index) => ({
                    name: method,
                    type: 'bar',
                    stack: 'total',
                    data: paginatedTaskIds.map(task_id => {
                        const val = taskIdMap[task_id].methods[method] || 0;
                        return { value: val, task_id: task_id };
                    }),
                    itemStyle: { color: getColor(index) },
                    label: { show: false }
                }));

                const taskLabels = paginatedTaskIds.map(task_id => task_id);

                const option = {
                    tooltip: {
                        trigger: 'axis',
                        axisPointer: { type: 'shadow' },
                        formatter: function(params) {
                            let tooltipText = params[0].name + '<br/>';
                            params.forEach(param => {
                                tooltipText += `${param.marker} ${param.seriesName}: ${param.value} ms<br/>`;
                            });
                            return tooltipText;
                        }
                    },
                    legend: {
                        data: methodNames,
                        top: 30,
                        textStyle: { fontSize: 14 }
                    },
                    toolbox: {
                        feature: { saveAsImage: {} }
                    },
                    grid: {
                        left: '3%',
                        right: '4%',
                        bottom: '15%',
                        containLabel: true
                    },
                    xAxis: {
                        type: 'category',
                        data: taskLabels,
                        axisLabel: { rotate: 45, interval: 0, fontSize: 12 },
                        axisLine: { lineStyle: { color: '#333' } },
                        axisTick: { alignWithLabel: true }
                    },
                    yAxis: {
                        type: 'value',
                        name: '耗时 (ms)',
                        axisLabel: { formatter: '{value} ms', fontSize: 12 },
                        axisLine: { lineStyle: { color: '#333' } },
                        splitLine: { lineStyle: { type: 'dashed', color: '#ccc' } }
                    },
                    dataZoom: [
                        { type: 'slider', xAxisIndex: 0, start: 0, end: 100, handleSize: '80%', height: 20, bottom: 20 },
                        { type: 'inside', xAxisIndex: 0, start: 0, end: 100 }
                    ],
                    series: series
                };

                const chartDom = document.getElementById('chart');
                const myChart = echarts.init(chartDom);
                myChart.setOption(option);
                window.addEventListener('resize', () => { myChart.resize(); });
                document.getElementById('loading').style.display = 'none';
            }

            async function updateChartPaginated() {
                const data = await fetchData();
                renderChartPaginated(data, currentPage, pageSize);
                addPaginationControls(data.total_task_ids, pageSize);
            }

            async function updateChart() {
                const data = await fetchData();
                renderChartPaginated(data, currentPage, pageSize);
                addPaginationControls(data.total_task_ids, pageSize);
            }

            updateChart();
            setInterval(updateChart, 5000);
        </script>

    </body>
    </html>
    "#;

    Ok(warp::reply::html(html))
}

pub async fn check_server_connection(){
    let client=Client::new();
    let url="http://localhost:10819";
    match client.get(url).send().await{
        Ok(response)=>{
            if response.status().is_success(){
                info!("连接成功:{} 状态:{:?}",url,response.status());
            }else{
                error!("连接失败,状态:{:?}",response.status());
            }
        },
        Err(e)=>{
            error!("无法连接服务器{}:{:?}",url,e);
        }
    }
}
